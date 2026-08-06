use std::{
    collections::HashMap,
    fs::OpenOptions,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{
    entities::auth::{KerberosCredentials, LdapIdentity, Session, SessionId},
    errors::AppError,
    services::kerberos::KerberosService,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};

/// Хранилище локальных сессий в памяти с debug-only восстановлением из файла.
///
/// Cookie содержит только `SessionId`. Debug snapshot содержит principal, путь
/// к ccache и сроки, но никогда не содержит исходный пароль или Kerberos ticket.
pub(crate) struct SessionService {
    /// Сессии индексируются только по непрозрачному значению из cookie.
    store: RwLock<HashMap<SessionId, Session>>,
    /// Единый срок действия новых сессий из конфигурации приложения.
    ttl: Duration,
    /// Управляет FILE ccache при удалении и восстановлении сессий.
    kerberos: Arc<KerberosService>,
    /// Debug-only файл, позволяющий пережить перезапуск backend во время разработки.
    persistence_path: Option<PathBuf>,
    /// Не допускает гонки нескольких атомарных записей одного snapshot-файла.
    persistence_lock: Mutex<()>,
}

#[derive(Serialize, Deserialize)]
struct PersistedSession {
    id: String,
    username: String,
    identifier: String,
    principal: String,
    ccache_path: PathBuf,
    tgt_expires_at_unix_ms: u64,
    expires_at_unix_ms: u64,
}

impl SessionService {
    /// Создаёт пустое хранилище сессий в памяти.
    pub(crate) fn new(ttl: Duration, kerberos: Arc<KerberosService>) -> Self {
        let persistence_path = cfg!(all(debug_assertions, not(test)))
            .then(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".dev-sessions.json"));
        Self::with_persistence(ttl, kerberos, persistence_path)
    }

    /// Создаёт хранилище и, если задан путь, восстанавливает debug-сессии с диска.
    fn with_persistence(
        ttl: Duration,
        kerberos: Arc<KerberosService>,
        persistence_path: Option<PathBuf>,
    ) -> Self {
        let store = persistence_path
            .as_deref()
            .map(|path| load_sessions(path, &kerberos))
            .unwrap_or_default();
        if let Some(path) = &persistence_path {
            tracing::info!(
                path = %path.display(),
                restored_sessions = store.len(),
                "сохранение отладочных сессий включено"
            );
        }

        Self {
            store: RwLock::new(store),
            ttl,
            kerberos,
            persistence_path,
            persistence_lock: Mutex::new(()),
        }
    }

    /// Создаёт локальную сессию, срок которой ограничен expiry Kerberos TGT.
    pub(crate) async fn create(
        &self,
        id: SessionId,
        identity: LdapIdentity,
        kerberos_credentials: KerberosCredentials,
    ) -> Result<Session, AppError> {
        let now = SystemTime::now();
        let configured_expires_at = now.checked_add(self.ttl).ok_or(AppError::Internal)?;
        let expires_at = configured_expires_at.min(kerberos_credentials.tgt_expires_at());
        if expires_at <= now {
            return Err(AppError::Unauthorized);
        }
        let session = Session {
            username: identity.username,
            kerberos_credentials: Arc::new(kerberos_credentials),
            expires_at,
        };

        let mut store = self.store.write().await;
        if store.contains_key(&id) {
            return Err(AppError::Internal);
        }

        store.insert(id, session.clone());
        drop(store);
        self.persist().await;

        Ok(session)
    }

    /// Возвращает неистёкшую сессию по непрозрачному идентификатору.
    pub(crate) async fn get(&self, id: &SessionId) -> Result<Session, AppError> {
        let mut store = self.store.write().await;
        let Some(session) = store.get(id) else {
            return Err(AppError::Unauthorized);
        };

        if session.expires_at <= SystemTime::now() {
            let expired = store.remove(id).ok_or(AppError::Unauthorized)?;
            tracing::info!(
                username = %expired.username,
                active_sessions = store.len(),
                "просроченная локальная сессия удалена во время поиска"
            );
            drop(store);
            self.kerberos
                .destroy_cache(&expired.kerberos_credentials)
                .await;
            self.persist().await;
            return Err(AppError::Unauthorized);
        }

        Ok(session.clone())
    }

    /// Удаляет и возвращает локальную сессию.
    pub(crate) async fn remove(&self, id: &SessionId) -> Option<Session> {
        let mut store = self.store.write().await;
        let removed = store.remove(id);
        drop(store);
        if let Some(session) = &removed {
            self.kerberos
                .destroy_cache(&session.kerberos_credentials)
                .await;
            self.persist().await;
        }
        removed
    }

    /// Удаляет сессии с истёкшим локальным сроком действия.
    pub(crate) async fn cleanup_expired(&self) {
        let now = SystemTime::now();
        let mut store = self.store.write().await;
        let expired_ids = store
            .iter()
            .filter_map(|(id, session)| (session.expires_at <= now).then_some(id.clone()))
            .collect::<Vec<_>>();
        let removed = expired_ids
            .into_iter()
            .filter_map(|id| store.remove(&id))
            .collect::<Vec<_>>();
        let changed = !removed.is_empty();
        drop(store);
        if changed {
            tracing::info!(
                removed_sessions = removed.len(),
                "просроченные сессии удалены"
            );
            for session in &removed {
                self.kerberos
                    .destroy_cache(&session.kerberos_credentials)
                    .await;
            }
            self.persist().await;
        }
    }

    /// Атомарно сохраняет допустимый debug-снимок сессий.
    async fn persist(&self) {
        let Some(path) = self.persistence_path.clone() else {
            return;
        };
        let _guard = self.persistence_lock.lock().await;
        let snapshot = {
            let store = self.store.read().await;
            persisted_snapshot(&store)
        };
        let result = tokio::task::spawn_blocking(move || write_sessions(&path, &snapshot)).await;
        match result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::warn!(%error, "не удалось сохранить отладочные сессии"),
            Err(error) => {
                tracing::warn!(%error, "фоновая задача сохранения отладочных сессий завершилась ошибкой")
            }
        }
    }
}

/// Преобразует in-memory сессии в сериализуемый debug-снимок без паролей и ticket-ов.
fn persisted_snapshot(store: &HashMap<SessionId, Session>) -> Vec<PersistedSession> {
    store
        .iter()
        .filter_map(|(id, session)| {
            let expires_at_unix_ms = session
                .expires_at
                .duration_since(UNIX_EPOCH)
                .ok()?
                .as_millis()
                .try_into()
                .ok()?;
            let tgt_expires_at_unix_ms = session
                .kerberos_credentials
                .tgt_expires_at()
                .duration_since(UNIX_EPOCH)
                .ok()?
                .as_millis()
                .try_into()
                .ok()?;
            Some(PersistedSession {
                id: id.to_string(),
                username: session.username.clone(),
                identifier: session.kerberos_credentials.identifier().to_owned(),
                principal: session.kerberos_credentials.principal().to_owned(),
                ccache_path: session.kerberos_credentials.ccache_path().to_owned(),
                tgt_expires_at_unix_ms,
                expires_at_unix_ms,
            })
        })
        .collect()
}

/// Загружает и валидирует debug-сессии, отбрасывая истёкшие или неполные ccache.
fn load_sessions(path: &Path, kerberos: &KerberosService) -> HashMap<SessionId, Session> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return HashMap::new(),
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "не удалось прочитать отладочные сессии");
            return HashMap::new();
        }
    };
    let persisted: Vec<PersistedSession> = match serde_json::from_slice(&bytes) {
        Ok(persisted) => persisted,
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "не удалось разобрать отладочные сессии");
            if let Err(remove_error) = std::fs::remove_file(path) {
                tracing::warn!(path = %path.display(), %remove_error, "не удалось удалить несовместимый снимок отладочных сессий");
            }
            return HashMap::new();
        }
    };

    let now = SystemTime::now();
    persisted
        .into_iter()
        .filter_map(|session| {
            let id = session.id.parse::<SessionId>().ok()?;
            let expires_at =
                UNIX_EPOCH.checked_add(Duration::from_millis(session.expires_at_unix_ms))?;
            let tgt_expires_at =
                UNIX_EPOCH.checked_add(Duration::from_millis(session.tgt_expires_at_unix_ms))?;
            let credentials = KerberosCredentials::new(
                session.identifier,
                session.principal,
                session.ccache_path,
                tgt_expires_at,
            );
            (expires_at > now && kerberos.is_restorable(&id, &credentials)).then(|| {
                (
                    id,
                    Session {
                        username: session.username,
                        kerberos_credentials: Arc::new(credentials),
                        expires_at,
                    },
                )
            })
        })
        .collect()
}

/// Записывает debug-снимок через временный файл и атомарное переименование.
fn write_sessions(path: &Path, sessions: &[PersistedSession]) -> io::Result<()> {
    let bytes = serde_json::to_vec(sessions).map_err(io::Error::other)?;
    let temporary_path = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let result = (|| {
        let mut file = options.open(&temporary_path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        std::fs::rename(&temporary_path, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary_path);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestFiles {
        persistence: PathBuf,
        ccache_dir: PathBuf,
    }

    impl TestFiles {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("sgu-priemka-sessions-{}", uuid::Uuid::new_v4()));
            Self {
                persistence: root.with_extension("json"),
                ccache_dir: root.join("krb5"),
            }
        }

        fn kerberos(&self) -> Arc<KerberosService> {
            Arc::new(KerberosService::for_tests(self.ccache_dir.clone()))
        }

        fn credentials(&self, id: &SessionId) -> KerberosCredentials {
            let ccache_path = self.ccache_dir.join(format!("{id}.ccache"));
            std::fs::write(&ccache_path, b"test ccache")
                .expect("тестовый ccache должен записываться");
            KerberosCredentials::new(
                "admin".to_owned(),
                "admin@MAIN.SGU.RU".to_owned(),
                ccache_path,
                SystemTime::now() + Duration::from_secs(120),
            )
        }
    }

    impl Drop for TestFiles {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.persistence);
            let _ = std::fs::remove_dir_all(
                self.ccache_dir
                    .parent()
                    .expect("тестовый ccache имеет parent"),
            );
        }
    }

    #[tokio::test]
    async fn restores_session_with_credentials_from_debug_file() {
        let files = TestFiles::new();
        let kerberos = files.kerberos();
        let id = SessionId::new();
        let credentials = files.credentials(&id);
        let service = SessionService::with_persistence(
            Duration::from_secs(60),
            kerberos.clone(),
            Some(files.persistence.clone()),
        );
        service
            .create(
                id.clone(),
                LdapIdentity {
                    username: "admin".to_owned(),
                },
                credentials,
            )
            .await
            .expect("сессия должна создаться");

        let restored = SessionService::with_persistence(
            Duration::from_secs(60),
            kerberos,
            Some(files.persistence.clone()),
        )
        .get(&id)
        .await
        .expect("сессия должна восстановиться");

        assert_eq!(restored.username, "admin");
        assert_eq!(restored.kerberos_credentials.identifier(), "admin");
        assert_eq!(
            restored.kerberos_credentials.principal(),
            "admin@MAIN.SGU.RU"
        );
        let persisted =
            std::fs::read_to_string(&files.persistence).expect("snapshot должен читаться");
        assert!(!persisted.contains("password"));
        assert!(!persisted.contains("secret"));
    }

    #[tokio::test]
    async fn removed_session_is_removed_from_debug_file() {
        let files = TestFiles::new();
        let kerberos = files.kerberos();
        let id = SessionId::new();
        let credentials = files.credentials(&id);
        let cache_path = credentials.ccache_path().to_owned();
        let service = SessionService::with_persistence(
            Duration::from_secs(60),
            kerberos.clone(),
            Some(files.persistence.clone()),
        );
        service
            .create(
                id.clone(),
                LdapIdentity {
                    username: "admin".to_owned(),
                },
                credentials,
            )
            .await
            .expect("сессия должна создаться");
        service.remove(&id).await;
        assert!(!cache_path.exists());

        let restored = SessionService::with_persistence(
            Duration::from_secs(60),
            kerberos,
            Some(files.persistence.clone()),
        );
        assert!(matches!(
            restored.get(&id).await,
            Err(AppError::Unauthorized)
        ));
    }
}
