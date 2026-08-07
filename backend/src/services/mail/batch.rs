use crate::errors::MailError;

/// Письмо, подготовленное orchestration-слоем до передачи SMTP-сервису.
#[derive(Clone, Debug)]
pub(crate) struct PreparedMail {
    /// Стабильный идентификатор строки исходного CSV.
    pub(crate) row_id: String,
    /// Адрес получателя.
    pub(crate) recipient: String,
    /// HTML-версия письма.
    pub(crate) html_body: String,
    /// Plain-text-версия письма.
    pub(crate) plain_text_body: String,
}

/// Результат отправки одному получателю.
#[derive(Debug)]
pub(crate) struct MailDeliveryResult {
    /// Идентификатор строки CSV.
    pub(crate) row_id: String,
    /// Адрес получателя.
    pub(crate) email: String,
    /// Состояние SMTP-операции.
    pub(crate) status: MailDeliveryStatus,
}

/// Результат принятия или отклонения письма SMTP-сервером.
#[derive(Debug)]
pub(crate) enum MailDeliveryStatus {
    /// SMTP принял письмо для дальнейшей доставки.
    AcceptedBySmtp,
    /// Отправка завершилась ошибкой.
    Failed {
        /// Можно ли безопасно повторить отправку.
        retryable: bool,
        /// Диагностическое описание для job и логов.
        reason: String,
    },
}

impl From<(String, String, MailError)> for MailDeliveryResult {
    fn from((row_id, email, error): (String, String, MailError)) -> Self {
        let retryable = matches!(
            error,
            MailError::ConnectionFailed { .. }
                | MailError::TemporaryFailure { .. }
                | MailError::Timeout
        );
        Self {
            row_id,
            email,
            status: MailDeliveryStatus::Failed {
                retryable,
                reason: error.to_string(),
            },
        }
    }
}
