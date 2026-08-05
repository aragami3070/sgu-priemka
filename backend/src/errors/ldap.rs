use ldap3::LdapError as Ldap3Error;
use thiserror::Error;

use super::KerberosError;

/// LDAP-поиск, во время которого возникла ошибка или неожиданный ответ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LdapOperation {
    /// Поиск вошедшего администратора и проверка группы `csit_admins`.
    AuthorizeUser,
    /// Поиск существующего студента по `sAMAccountName`.
    SearchStudent,
}

/// Этап изменения LDAP, на котором могла частично создаться учётная запись.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LdapPhase {
    /// Создание объекта внутри настроенного контейнера.
    AddObject,
    /// Установка сгенерированного пароля после создания объекта.
    SetPassword,
    /// Включение учётной записи после установки пароля.
    EnableAccount,
}

/// Единая ошибка LDAP-аутентификации и операций с учётными записями.
#[derive(Debug, Error)]
pub(crate) enum LdapError {
    /// LDAP отклонил credentials пользователя.
    #[error("invalid LDAP credentials")]
    InvalidCredentials,
    /// Не удалось получить explicit GSSAPI credential из session ccache.
    #[error("Kerberos credential preparation for LDAP failed")]
    Kerberos {
        #[source]
        source: KerberosError,
    },
    /// Учётная запись не состоит в разрешённой группе.
    #[error("LDAP account is not allowed to use the service")]
    Forbidden,
    /// Не удалось открыть соединение с LDAP.
    #[error("LDAP connection failed")]
    Connect {
        /// Исходная ошибка ldap3.
        #[source]
        source: Ldap3Error,
    },
    /// Не удалось выполнить LDAP-аутентификацию на транспортном уровне.
    #[error("LDAP authentication transport failed")]
    AuthenticationTransport {
        /// Исходная ошибка ldap3.
        #[source]
        source: Ldap3Error,
    },
    /// LDAP-сервер отклонил аутентификацию с result code, отличным от 49.
    #[error("LDAP authentication rejected with code {result_code}: {diagnostic}")]
    AuthenticationRejected {
        /// LDAP result code ответа сервера.
        result_code: u32,
        /// Диагностический текст LDAP-ответа.
        diagnostic: String,
    },
    /// LDAP-поиск не был выполнен или отклонён сервером.
    #[error("LDAP search failed during {operation:?}")]
    Search {
        /// Прикладной поиск, завершившийся ошибкой.
        operation: LdapOperation,
        /// Исходная ошибка ldap3.
        #[source]
        source: Ldap3Error,
    },
    /// LDAP-поиск вернул неоднозначное количество записей.
    #[error("LDAP search during {operation:?} returned {actual} entries, expected {expected}")]
    UnexpectedSearchResult {
        /// Прикладной поиск, вернувший неожиданный ответ.
        operation: LdapOperation,
        /// Понятное описание ожидаемого количества.
        expected: &'static str,
        /// Фактическое количество записей.
        actual: usize,
    },
    /// В найденной записи отсутствует обязательный атрибут или DN.
    #[error("LDAP result during {operation:?} has no valid `{attribute}`")]
    MissingAttribute {
        /// Прикладной поиск, из которого извлекалась запись.
        operation: LdapOperation,
        /// Имя обязательного LDAP-атрибута.
        attribute: &'static str,
    },
    /// Изменение LDAP завершилось ошибкой и могло оставить частичный результат.
    #[error("LDAP operation failed at {phase:?}: {message}")]
    Operation {
        /// Этап изменения LDAP.
        phase: LdapPhase,
        /// Может ли LDAP-объект уже существовать несмотря на ошибку.
        possibly_created: bool,
        /// Техническая причина для логов и отчёта.
        message: String,
    },
}

impl From<KerberosError> for LdapError {
    fn from(source: KerberosError) -> Self {
        Self::Kerberos { source }
    }
}

impl LdapError {
    /// Создаёт ошибку транспорта при открытии соединения.
    pub(crate) fn connect(source: Ldap3Error) -> Self {
        Self::Connect { source }
    }

    /// Создаёт ошибку транспорта LDAP-аутентификации.
    pub(crate) fn authentication_transport(source: Ldap3Error) -> Self {
        Self::AuthenticationTransport { source }
    }

    /// Создаёт ошибку LDAP-поиска с контекстом прикладной операции.
    pub(crate) fn search(operation: LdapOperation, source: Ldap3Error) -> Self {
        Self::Search { operation, source }
    }
}
