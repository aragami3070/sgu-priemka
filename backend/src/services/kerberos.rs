use std::{
    ffi::{CStr, CString},
    fs,
    mem::MaybeUninit,
    os::raw::c_char,
    path::{Path, PathBuf},
    ptr,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use cross_krb5::Cred as GssapiCredential;
use krb5_sys::{
    KRB5_LIBOS_BADPWDMATCH, KRB5_PREAUTH_FAILED, KRB5KDC_ERR_C_PRINCIPAL_UNKNOWN,
    KRB5KDC_ERR_CLIENT_REVOKED, KRB5KDC_ERR_KEY_EXP, KRB5KDC_ERR_PREAUTH_FAILED, krb5_cc_close,
    krb5_cc_destroy, krb5_cc_initialize, krb5_cc_resolve, krb5_cc_store_cred, krb5_ccache,
    krb5_context, krb5_creds, krb5_error_code, krb5_free_context, krb5_free_cred_contents,
    krb5_free_error_message, krb5_free_principal, krb5_get_error_message,
    krb5_get_init_creds_password, krb5_init_context, krb5_keytab, krb5_parse_name, krb5_principal,
};
use libgssapi::{credential::Cred as LibGssapiCredential, error::MajorFlags};
use libgssapi_sys::{GSS_S_COMPLETE, OM_uint32, gss_cred_id_t};
use zeroize::Zeroizing;

use crate::{
    config::Config,
    entities::auth::{KerberosCredentials, SessionId},
    errors::KerberosError,
};

#[link(name = "gssapi_krb5")]
unsafe extern "C" {
    /// Импортирует credential из конкретного MIT Kerberos credential cache.
    fn gss_krb5_import_cred(
        minor_status: *mut OM_uint32,
        cache: krb5_ccache,
        keytab_principal: krb5_principal,
        keytab: krb5_keytab,
        credential: *mut gss_cred_id_t,
    ) -> OM_uint32;
}

/// Получает пользовательские TGT и импортирует explicit GSSAPI credentials.
pub(crate) struct KerberosService {
    realm: String,
    ccache_dir: PathBuf,
}

impl KerberosService {
    /// Проверяет и закрывает каталог, в котором живут персональные FILE ccache.
    pub(crate) fn new(config: Arc<Config>) -> Result<Self, KerberosError> {
        prepare_cache_directory(&config.kerberos.ccache_dir)?;
        let ccache_dir = fs::canonicalize(&config.kerberos.ccache_dir)
            .map_err(|source| KerberosError::cache_io(&config.kerberos.ccache_dir, source))?;
        if !cfg!(debug_assertions) {
            cleanup_release_caches(&ccache_dir)?;
        }
        tracing::info!(
            realm = %config.kerberos.realm,
            ccache_dir = %ccache_dir.display(),
            "сервис Kerberos инициализирован"
        );
        Ok(Self {
            realm: config.kerberos.realm.clone(),
            ccache_dir,
        })
    }

    /// Получает TGT по паролю и сохраняет его в ccache, привязанный к session ID.
    pub(crate) async fn acquire_tgt(
        &self,
        identifier: String,
        password: Zeroizing<String>,
        session_id: &SessionId,
    ) -> Result<KerberosCredentials, KerberosError> {
        let identifier = identifier.trim().to_owned();
        if identifier.is_empty() || password.trim().is_empty() {
            return Err(KerberosError::InvalidCredentials);
        }

        let principal = format!("{}@{}", identifier, self.realm);
        let ccache_path = self.cache_path(session_id);
        let task_principal = principal.clone();
        let task_path = ccache_path.clone();
        let tgt_expires_at = tokio::task::spawn_blocking(move || {
            acquire_tgt_blocking(&task_principal, password, &task_path)
        })
        .await
        .map_err(|source| KerberosError::BlockingTask { source })??;

        Ok(KerberosCredentials::new(
            identifier,
            principal,
            ccache_path,
            tgt_expires_at,
        ))
    }

    /// Импортирует credential конкретного ccache, не изменяя `KRB5CCNAME` процесса.
    pub(crate) async fn gssapi_credential(
        &self,
        credentials: &KerberosCredentials,
    ) -> Result<GssapiCredential, KerberosError> {
        let path = credentials.ccache_path().to_owned();
        let credential = tokio::task::spawn_blocking(move || import_gssapi_credential(&path))
            .await
            .map_err(|source| KerberosError::BlockingTask { source })??;
        Ok(credential)
    }

    /// Удаляет FILE ccache после logout, expiry или неуспешной LDAP-авторизации.
    pub(crate) async fn destroy_cache(&self, credentials: &KerberosCredentials) {
        let path = credentials.ccache_path().to_owned();
        match tokio::fs::remove_file(&path).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                tracing::warn!(ccache_path = %path.display(), %error, "не удалось удалить Kerberos ccache")
            }
        }
    }

    /// Проверяет debug-сессию перед восстановлением после перезапуска.
    pub(crate) fn is_restorable(
        &self,
        session_id: &SessionId,
        credentials: &KerberosCredentials,
    ) -> bool {
        let metadata_is_valid = credentials.ccache_path() == self.cache_path(session_id)
            && credentials.tgt_expires_at() > SystemTime::now()
            && credentials.ccache_path().is_file();
        if !metadata_is_valid {
            return false;
        }
        if cfg!(test) {
            true
        } else {
            import_gssapi_credential(credentials.ccache_path()).is_ok()
        }
    }

    /// Возвращает путь к ccache конкретной серверной сессии.
    fn cache_path(&self, session_id: &SessionId) -> PathBuf {
        self.ccache_dir.join(format!("{session_id}.ccache"))
    }

    #[cfg(test)]
    pub(crate) fn for_tests(ccache_dir: PathBuf) -> Self {
        prepare_cache_directory(&ccache_dir).expect("тестовый ccache-каталог должен создаваться");
        let ccache_dir = fs::canonicalize(ccache_dir)
            .expect("тестовый ccache-каталог должен иметь абсолютный путь");
        Self {
            realm: "MAIN.SGU.RU".to_owned(),
            ccache_dir,
        }
    }
}

/// Release не восстанавливает локальные сессии, поэтому удаляет оставшиеся session ccache.
fn cleanup_release_caches(directory: &Path) -> Result<(), KerberosError> {
    let entries =
        fs::read_dir(directory).map_err(|source| KerberosError::cache_io(directory, source))?;
    for entry in entries {
        let entry = entry.map_err(|source| KerberosError::cache_io(directory, source))?;
        let path = entry.path();
        let is_session_cache = path
            .extension()
            .is_some_and(|extension| extension == "ccache")
            && path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .is_some_and(|stem| uuid::Uuid::parse_str(stem).is_ok());
        if is_session_cache {
            fs::remove_file(&path).map_err(|source| KerberosError::cache_io(&path, source))?;
        }
    }
    Ok(())
}

/// Создаёт dedicated ccache-каталог и запрещает использовать symlink/обычный файл.
fn prepare_cache_directory(path: &Path) -> Result<(), KerberosError> {
    fs::create_dir_all(path).map_err(|source| KerberosError::cache_io(path, source))?;
    let metadata =
        fs::symlink_metadata(path).map_err(|source| KerberosError::cache_io(path, source))?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(KerberosError::cache_io(
            path,
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "ccache path must be a real directory",
            ),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|source| KerberosError::cache_io(path, source))?;
    }
    Ok(())
}

fn acquire_tgt_blocking(
    principal_name: &str,
    password: Zeroizing<String>,
    ccache_path: &Path,
) -> Result<SystemTime, KerberosError> {
    if ccache_path
        .try_exists()
        .map_err(|source| KerberosError::cache_io(ccache_path, source))?
    {
        return Err(KerberosError::CacheAlreadyExists(ccache_path.to_owned()));
    }

    let context = KrbContext::new()?;
    let principal_name = c_string(principal_name, "principal")?;
    let principal = KrbPrincipal::parse(&context, &principal_name)?;
    let mut password_bytes = Zeroizing::new(password.as_bytes().to_vec());
    if password_bytes.contains(&0) {
        return Err(KerberosError::InteriorNul { field: "password" });
    }
    password_bytes.push(0);

    let mut credentials = InitialCredentials::empty(&context);
    // SAFETY: context/principal действительны; credentials указывает на нулевую C-структуру,
    // password_bytes завершается NUL и живёт до возврата функции.
    let code = unsafe {
        krb5_get_init_creds_password(
            context.raw(),
            credentials.raw_mut(),
            principal.raw(),
            password_bytes.as_ptr().cast::<c_char>(),
            None,
            ptr::null_mut(),
            0,
            ptr::null(),
            ptr::null(),
        )
    };
    if code != 0 {
        return Err(context.operation_error("get initial credentials", code));
    }
    credentials.mark_initialized();

    let cache_name = cache_name(ccache_path)?;
    let mut cache = KrbCache::resolve(&context, &cache_name, true)?;
    cache.call(
        "initialize credential cache",
        // SAFETY: cache и principal принадлежат действующему context.
        unsafe { krb5_cc_initialize(context.raw(), cache.raw(), principal.raw()) },
    )?;
    cache.call(
        "store initial credentials",
        // SAFETY: cache и credentials действительны до завершения вызова.
        unsafe { krb5_cc_store_cred(context.raw(), cache.raw(), credentials.raw_mut()) },
    )?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(ccache_path, fs::Permissions::from_mode(0o600))
            .map_err(|source| KerberosError::cache_io(ccache_path, source))?;
    }

    let endtime = credentials.endtime();
    let endtime = u64::try_from(endtime).map_err(|_| KerberosError::InvalidExpiration)?;
    let expires_at = UNIX_EPOCH
        .checked_add(std::time::Duration::from_secs(endtime))
        .ok_or(KerberosError::InvalidExpiration)?;
    if expires_at <= SystemTime::now() {
        return Err(KerberosError::InvalidExpiration);
    }

    cache.keep();
    Ok(expires_at)
}

fn import_gssapi_credential(ccache_path: &Path) -> Result<GssapiCredential, KerberosError> {
    let context = KrbContext::new()?;
    let cache_name = cache_name(ccache_path)?;
    let cache = KrbCache::resolve(&context, &cache_name, false)?;
    let mut minor = GSS_S_COMPLETE;
    let mut raw_credential: gss_cred_id_t = ptr::null_mut();
    // SAFETY: cache открыт и живёт до конца вызова; остальные optional handles намеренно NULL;
    // функция записывает новый credential handle в raw_credential.
    let major = unsafe {
        gss_krb5_import_cred(
            &mut minor,
            cache.raw(),
            ptr::null_mut(),
            ptr::null_mut(),
            &mut raw_credential,
        )
    };
    if major != GSS_S_COMPLETE || raw_credential.is_null() {
        let error = libgssapi::error::Error {
            major: MajorFlags::from_bits_retain(major),
            minor,
        };
        return Err(KerberosError::Gssapi {
            message: error.to_string(),
        });
    }

    let credential = LibGssapiCredential::from(raw_credential);
    Ok(GssapiCredential::from(credential))
}

fn cache_name(path: &Path) -> Result<CString, KerberosError> {
    let path = path.to_str().ok_or_else(|| {
        KerberosError::cache_io(
            path,
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "ccache path is not UTF-8"),
        )
    })?;
    c_string(&format!("FILE:{path}"), "ccache path")
}

fn c_string(value: &str, field: &'static str) -> Result<CString, KerberosError> {
    CString::new(value).map_err(|_| KerberosError::InteriorNul { field })
}

fn invalid_credential_code(code: krb5_error_code) -> bool {
    matches!(
        code,
        KRB5KDC_ERR_C_PRINCIPAL_UNKNOWN
            | KRB5KDC_ERR_CLIENT_REVOKED
            | KRB5KDC_ERR_KEY_EXP
            | KRB5KDC_ERR_PREAUTH_FAILED
            | KRB5_PREAUTH_FAILED
            | KRB5_LIBOS_BADPWDMATCH
    )
}

struct KrbContext(krb5_context);

impl KrbContext {
    /// Создаёт контекст MIT Kerberos.
    fn new() -> Result<Self, KerberosError> {
        let mut raw = ptr::null_mut();
        // SAFETY: raw указывает на место для нового context handle.
        let code = unsafe { krb5_init_context(&mut raw) };
        if code == 0 && !raw.is_null() {
            Ok(Self(raw))
        } else {
            Err(KerberosError::Library {
                operation: "initialize context",
                code,
                message: "MIT Kerberos context initialization failed".to_owned(),
            })
        }
    }

    /// Возвращает заимствованный raw handle для FFI-вызовов.
    fn raw(&self) -> krb5_context {
        self.0
    }

    /// Преобразует код MIT Kerberos в типизированную ошибку с диагностикой.
    fn operation_error(&self, operation: &'static str, code: krb5_error_code) -> KerberosError {
        operation_error_for_context(self.0, operation, code)
    }
}

impl Drop for KrbContext {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: handle принадлежит этой обёртке и освобождается ровно один раз.
            unsafe { krb5_free_context(self.0) };
        }
    }
}

struct KrbPrincipal {
    context: krb5_context,
    raw: krb5_principal,
}

impl KrbPrincipal {
    /// Разбирает строковое имя principal в MIT handle.
    fn parse(context: &KrbContext, name: &CString) -> Result<Self, KerberosError> {
        let mut raw = ptr::null_mut();
        // SAFETY: context/name действительны, raw принимает новый principal handle.
        let code = unsafe { krb5_parse_name(context.raw(), name.as_ptr(), &mut raw) };
        if code == 0 && !raw.is_null() {
            Ok(Self {
                context: context.raw(),
                raw,
            })
        } else {
            Err(context.operation_error("parse principal", code))
        }
    }

    /// Возвращает заимствованный raw principal handle.
    fn raw(&self) -> krb5_principal {
        self.raw
    }
}

impl Drop for KrbPrincipal {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            // SAFETY: principal создан этим context и освобождается один раз до context.
            unsafe { krb5_free_principal(self.context, self.raw) };
        }
    }
}

struct InitialCredentials {
    context: krb5_context,
    raw: krb5_creds,
    initialized: bool,
}

impl InitialCredentials {
    /// Создаёт нулевую output-структуру для получения initial credentials.
    fn empty(context: &KrbContext) -> Self {
        // SAFETY: libkrb5 ожидает нулевую krb5_creds как output-структуру.
        let raw = unsafe { MaybeUninit::<krb5_creds>::zeroed().assume_init() };
        Self {
            context: context.raw(),
            raw,
            initialized: false,
        }
    }

    /// Возвращает изменяемый raw handle для FFI-вызова получения credentials.
    fn raw_mut(&mut self) -> *mut krb5_creds {
        &mut self.raw
    }

    /// Отмечает структуру как заполненную и требующую освобождения.
    fn mark_initialized(&mut self) {
        self.initialized = true;
    }

    /// Возвращает Unix-время окончания действия TGT.
    fn endtime(&self) -> i32 {
        self.raw.times.endtime
    }
}

impl Drop for InitialCredentials {
    fn drop(&mut self) {
        if self.initialized {
            // SAFETY: содержимое было инициализировано успешным get_init_creds и освобождается один раз.
            unsafe { krb5_free_cred_contents(self.context, &mut self.raw) };
        }
    }
}

struct KrbCache {
    context: krb5_context,
    raw: krb5_ccache,
    destroy_on_drop: bool,
}

impl KrbCache {
    /// Открывает FILE ccache и задаёт политику его закрытия при drop.
    fn resolve(
        context: &KrbContext,
        name: &CString,
        destroy_on_drop: bool,
    ) -> Result<Self, KerberosError> {
        let mut raw = ptr::null_mut();
        // SAFETY: context/name действительны, raw принимает новый cache handle.
        let code = unsafe { krb5_cc_resolve(context.raw(), name.as_ptr(), &mut raw) };
        if code == 0 && !raw.is_null() {
            Ok(Self {
                context: context.raw(),
                raw,
                destroy_on_drop,
            })
        } else {
            Err(context.operation_error("resolve credential cache", code))
        }
    }

    /// Возвращает заимствованный raw cache handle.
    fn raw(&self) -> krb5_ccache {
        self.raw
    }

    /// Проверяет код результата операции над cache.
    fn call(&self, operation: &'static str, code: krb5_error_code) -> Result<(), KerberosError> {
        if code == 0 {
            Ok(())
        } else {
            Err(operation_error_for_context(self.context, operation, code))
        }
    }

    /// Оставляет cache на диске после закрытия handle.
    fn keep(&mut self) {
        self.destroy_on_drop = false;
    }
}

impl Drop for KrbCache {
    fn drop(&mut self) {
        if self.raw.is_null() {
            return;
        }
        // SAFETY: cache handle принадлежит обёртке. destroy одновременно закрывает handle;
        // для сохранённого cache вызывается только close.
        unsafe {
            if self.destroy_on_drop {
                let _ = krb5_cc_destroy(self.context, self.raw);
            } else {
                let _ = krb5_cc_close(self.context, self.raw);
            }
        }
    }
}

/// Преобразует код MIT Kerberos, используя заимствованный context handle.
fn operation_error_for_context(
    context: krb5_context,
    operation: &'static str,
    code: krb5_error_code,
) -> KerberosError {
    if invalid_credential_code(code) {
        return KerberosError::InvalidCredentials;
    }
    // SAFETY: context живёт дольше вызова; сообщение освобождается парным libkrb5-вызовом.
    let message = unsafe {
        let raw = krb5_get_error_message(context, code);
        if raw.is_null() {
            "unknown MIT Kerberos error".to_owned()
        } else {
            let message = CStr::from_ptr(raw).to_string_lossy().into_owned();
            krb5_free_error_message(context, raw);
            message
        }
    };
    KerberosError::Library {
        operation,
        code,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_authentication_error_codes() {
        for code in [
            KRB5KDC_ERR_C_PRINCIPAL_UNKNOWN,
            KRB5KDC_ERR_CLIENT_REVOKED,
            KRB5KDC_ERR_KEY_EXP,
            KRB5KDC_ERR_PREAUTH_FAILED,
            KRB5_PREAUTH_FAILED,
            KRB5_LIBOS_BADPWDMATCH,
        ] {
            assert!(invalid_credential_code(code));
        }
    }

    #[test]
    fn builds_file_cache_name() {
        let name = cache_name(Path::new("/tmp/session.ccache")).expect("путь корректен");
        assert_eq!(name.to_bytes(), b"FILE:/tmp/session.ccache");
    }

    #[test]
    fn release_cleanup_removes_only_session_caches() {
        let directory = std::env::temp_dir().join(format!(
            "sgu-priemka-kerberos-cleanup-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&directory).expect("каталог должен создаваться");
        let session_cache = directory.join(format!("{}.ccache", uuid::Uuid::new_v4()));
        let unrelated = directory.join("manual.ccache");
        fs::write(&session_cache, b"cache").expect("session cache должен записываться");
        fs::write(&unrelated, b"cache").expect("сторонний файл должен записываться");

        cleanup_release_caches(&directory).expect("очистка должна завершиться");

        assert!(!session_cache.exists());
        assert!(unrelated.exists());
        fs::remove_dir_all(directory).expect("тестовый каталог должен удаляться");
    }
}
