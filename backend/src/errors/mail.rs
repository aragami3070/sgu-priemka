use thiserror::Error;

/// Ошибка подготовки или отправки письма.
#[derive(Debug, Error)]
pub(crate) enum MailError {
    /// Адрес получателя не удалось разобрать.
    #[error("некорректный адрес получателя `{email}`")]
    InvalidRecipient {
        /// Адрес, который не удалось разобрать.
        email: String,
        /// Исходная ошибка lettre.
        #[source]
        source: lettre::address::AddressError,
    },
    /// Письмо не удалось собрать.
    #[error("не удалось собрать письмо")]
    MessageBuild {
        /// Исходная ошибка lettre.
        #[source]
        source: lettre::error::Error,
    },
    /// Шаблон письма не удалось отрендерить.
    #[error("не удалось отрендерить шаблон письма: {reason}")]
    TemplateRender {
        /// Описание причины ошибки шаблонизатора.
        reason: String,
    },
    /// SMTP-конфигурация содержит противоречивые значения.
    #[error("некорректная SMTP-конфигурация: {0}")]
    InvalidConfig(String),
    /// Проверка SMTP-соединения завершилась неуспешно.
    #[error("проверка SMTP-соединения не пройдена")]
    ConnectionTestFailed,
    /// Не удалось подключиться к SMTP.
    #[error("не удалось подключиться к SMTP")]
    ConnectionFailed {
        /// Исходная ошибка SMTP-транспорта.
        #[source]
        source: lettre::transport::smtp::Error,
    },
    /// TLS-соединение не удалось установить.
    #[error("TLS-соединение не удалось установить")]
    TlsFailure {
        /// Исходная ошибка SMTP-транспорта.
        #[source]
        source: lettre::transport::smtp::Error,
    },
    /// SMTP-операция превысила настроенный таймаут.
    #[error("SMTP-операция превысила таймаут")]
    Timeout,
    /// SMTP временно отклонил письмо.
    #[error("SMTP временно отклонил письмо")]
    TemporaryFailure {
        /// Исходная ошибка SMTP-транспорта.
        #[source]
        source: lettre::transport::smtp::Error,
    },
    /// SMTP окончательно отклонил письмо.
    #[error("SMTP окончательно отклонил письмо")]
    PermanentFailure {
        /// Исходная ошибка SMTP-транспорта.
        #[source]
        source: lettre::transport::smtp::Error,
    },
}

impl MailError {
    /// Единственный источник истины: нужно ли повторять SMTP-операцию.
    pub(crate) fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::TemporaryFailure { .. } | Self::Timeout | Self::ConnectionFailed { .. }
        )
    }
}
