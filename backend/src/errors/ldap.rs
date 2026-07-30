use thiserror::Error;

/// Этап LDAP, на котором не удалось создать учётную запись.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LdapPhase {
    /// Создание объекта внутри настроенного контейнера.
    AddObject,
    /// Установка сгенерированного пароля после создания объекта.
    SetPassword,
    /// Включение учётной записи после установки пароля.
    EnableAccount,
}

/// Подробная ошибка создания учётной записи через LDAP.
#[derive(Clone, Debug, Error)]
#[error("ошибка LDAP на этапе {phase:?}: {message}")]
pub(crate) struct LdapError {
    /// Операция, завершившаяся ошибкой.
    pub(crate) phase: LdapPhase,
    /// Может ли LDAP-объект уже существовать несмотря на ошибку.
    pub(crate) possibly_created: bool,
    /// Диагностическое сообщение для логов и отчёта об ошибке.
    pub(crate) message: String,
}
