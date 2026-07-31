use std::time::Instant;

use uuid::Uuid;

/// Идентификатор пользователя после успешного LDAP bind и проверки `csit_admins`.
#[derive(Clone, Debug)]
pub(crate) struct LdapIdentity {
    /// Каноническое имя пользователя из LDAP-атрибута `sAMAccountName`.
    pub(crate) username: String,
}

/// Непрозрачный случайный идентификатор, хранящийся в cookie браузера.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SessionId(
    /// Исходное значение закрыто, чтобы нельзя было создать непроверенный идентификатор сессии.
    Uuid,
);

/// Локальная серверная сессия, не содержащая LDAP credentials.
#[derive(Clone, Debug)]
pub(crate) struct Session {
    /// Каноническое имя пользователя из `sAMAccountName`.
    pub(crate) username: String,
    /// UUID группы результатов, созданных в этой сессии.
    pub(crate) storage_id: Uuid,
    /// Момент, после которого локальная сессия считается истёкшей.
    pub(crate) expires_at: Instant,
}
