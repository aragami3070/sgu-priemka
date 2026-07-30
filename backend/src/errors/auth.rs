use thiserror::Error;

/// Ошибки пользовательской аутентификации и авторизации через LDAP.
#[derive(Debug, Error)]
pub(crate) enum LdapAuthError {
    /// LDAP отклонил сформированный Bind DN или введённый пароль.
    #[error("invalid LDAP credentials")]
    InvalidCredentials,
    /// Учётная запись не состоит в группе `csit_admins`.
    #[error("LDAP account is not allowed to use the service")]
    Forbidden,
    /// LDAP недоступен или не смог выполнить bind/search.
    #[error("LDAP is unavailable")]
    Unavailable,
    /// В разрешённой LDAP-записи отсутствует `sAMAccountName`.
    #[error("LDAP account has no sAMAccountName")]
    MissingSamAccountName,
    /// Поиск текущей учётной записи вернул неоднозначный результат.
    #[error("LDAP account search returned an unexpected result")]
    UnexpectedSearchResult,
}
