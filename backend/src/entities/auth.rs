use std::{fmt, str::FromStr, sync::Arc, time::Instant};

use uuid::Uuid;

/// Идентификатор пользователя после успешного LDAP bind и проверки `csit_admins`.
#[derive(Clone, Debug)]
pub(crate) struct LdapIdentity {
    /// Каноническое имя пользователя из LDAP-атрибута `sAMAccountName`.
    pub(crate) username: String,
}

/// Credentials вошедшего пользователя для повторных LDAP bind в рамках сессии.
///
/// Тип намеренно не реализует `Debug`, чтобы пароль нельзя было случайно вывести в лог.
pub(crate) struct LdapCredentials {
    /// Канонический `sAMAccountName`, полученный после проверки пользователя в LDAP.
    identifier: String,
    /// Пароль LDAP, хранящийся только в памяти backend до удаления сессии.
    password: String,
}

impl LdapCredentials {
    /// Создаёт credentials после успешной проверки LDAP bind.
    pub(crate) fn new(identifier: String, password: String) -> Self {
        Self {
            identifier,
            password,
        }
    }

    /// Возвращает канонический `sAMAccountName` для bind и каталога результатов.
    pub(crate) fn identifier(&self) -> &str {
        &self.identifier
    }

    /// Возвращает пароль только для выполнения пользовательского LDAP bind.
    pub(crate) fn password(&self) -> &str {
        &self.password
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

/// Локальная серверная сессия с credentials вошедшего LDAP-пользователя.
///
/// Тип намеренно не реализует `Debug`, чтобы credentials не могли попасть в лог.
#[derive(Clone)]
pub(crate) struct Session {
    /// Каноническое имя пользователя из `sAMAccountName`.
    pub(crate) username: String,
    /// Credentials используются всеми LDAP-операциями, запущенными из этой сессии.
    pub(crate) ldap_credentials: Arc<LdapCredentials>,
    /// Момент, после которого локальная сессия считается истёкшей.
    pub(crate) expires_at: Instant,
}
