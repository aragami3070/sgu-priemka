use std::{
    fmt,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
    time::SystemTime,
};

use uuid::Uuid;

/// Идентификатор пользователя после успешной LDAP-аутентификации и проверки `csit_admins`.
#[derive(Clone, Debug)]
pub(crate) struct LdapIdentity {
    /// Каноническое имя пользователя из LDAP-атрибута `sAMAccountName`.
    pub(crate) username: String,
}

/// Персональный Kerberos context вошедшего пользователя для explicit GSSAPI-аутентификации.
#[derive(Clone, Debug)]
pub(crate) struct KerberosCredentials {
    /// Канонический `sAMAccountName`, подтверждённый LDAP-проверкой доступа.
    identifier: String,
    /// Полный Kerberos principal пользователя.
    principal: String,
    /// Путь к изолированному FILE ccache этой сессии.
    ccache_path: PathBuf,
    /// Срок действия полученного TGT.
    tgt_expires_at: SystemTime,
}

impl KerberosCredentials {
    /// Создаёт проверенный Kerberos context после успешного получения TGT.
    pub(crate) fn new(
        identifier: String,
        principal: String,
        ccache_path: PathBuf,
        tgt_expires_at: SystemTime,
    ) -> Self {
        Self {
            identifier,
            principal,
            ccache_path,
            tgt_expires_at,
        }
    }

    /// Возвращает канонический `sAMAccountName` для LDAP-поиска и аудита.
    pub(crate) fn identifier(&self) -> &str {
        &self.identifier
    }

    /// Заменяет введённый identifier каноническим `sAMAccountName` из LDAP.
    pub(crate) fn set_identifier(&mut self, identifier: String) {
        self.identifier = identifier;
    }

    /// Возвращает полный principal, которому принадлежит ccache.
    pub(crate) fn principal(&self) -> &str {
        &self.principal
    }

    /// Возвращает путь к FILE ccache без изменения process-global окружения.
    pub(crate) fn ccache_path(&self) -> &Path {
        &self.ccache_path
    }

    /// Возвращает срок действия TGT для ограничения локальной сессии.
    pub(crate) fn tgt_expires_at(&self) -> SystemTime {
        self.tgt_expires_at
    }

    #[cfg(test)]
    pub(crate) fn for_tests(identifier: impl Into<String>) -> Self {
        let identifier = identifier.into();
        Self::new(
            identifier.clone(),
            format!("{identifier}@MAIN.SGU.RU"),
            PathBuf::from("/tmp/test.ccache"),
            SystemTime::now() + std::time::Duration::from_secs(3600),
        )
    }
}

/// Непрозрачный случайный идентификатор, хранящийся в cookie браузера.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct SessionId(
    /// Исходное значение закрыто, чтобы нельзя было создать непроверенный идентификатор сессии.
    Uuid,
);

impl SessionId {
    /// Создаёт новый криптографически случайный UUID v4 для локальной сессии.
    pub(crate) fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for SessionId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(value).map(Self)
    }
}

/// Локальная серверная сессия с персональным Kerberos context пользователя.
#[derive(Clone)]
pub(crate) struct Session {
    /// Каноническое имя пользователя из `sAMAccountName`.
    pub(crate) username: String,
    /// Kerberos context используется для explicit GSSAPI-аутентификации от имени этой сессии.
    pub(crate) kerberos_credentials: Arc<KerberosCredentials>,
    /// Момент, после которого локальная сессия считается истёкшей.
    pub(crate) expires_at: SystemTime,
}
