use std::{sync::Arc, time::Duration};

use futures_util::{StreamExt, stream};
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Mailbox, MultiPart, header::ContentType},
    transport::smtp::authentication::Credentials,
};

use crate::{
    config::{MailConfig, SmtpSecurity},
    errors::MailError,
};

use super::{
    batch::{MailDeliveryResult, MailDeliveryStatus, PreparedMail},
    template::{CredentialTemplateData, RenderedMail, TemplateService},
};

/// Переиспользуемый SMTP-сервис с общим connection pool.
#[derive(Clone)]
pub(crate) struct MailService {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
    subject: Arc<str>,
    templates: TemplateService,
    max_concurrent: usize,
    timeout: Duration,
}

impl MailService {
    /// Создаёт SMTP-транспорт один раз при запуске приложения.
    pub(crate) fn new(config: &MailConfig) -> Result<Self, MailError> {
        let from = Mailbox::new(
            Some(config.from_name.clone()),
            config
                .from_address
                .parse()
                .map_err(|source| MailError::InvalidRecipient {
                    email: config.from_address.clone(),
                    source,
                })?,
        );
        let timeout = Duration::from_secs(config.timeout_seconds);
        let port = config.smtp_port.unwrap_or(match config.smtp_security {
            SmtpSecurity::StartTls => 587,
            SmtpSecurity::ImplicitTls => 465,
        });
        let builder = match config.smtp_security {
            SmtpSecurity::StartTls => {
                AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.smtp_host)
                    .map_err(|source| MailError::ConnectionFailed { source })?
            }
            SmtpSecurity::ImplicitTls => {
                AsyncSmtpTransport::<Tokio1Executor>::relay(&config.smtp_host)
                    .map_err(|source| MailError::ConnectionFailed { source })?
            }
        };
        let mut builder = builder.port(port).timeout(Some(timeout));
        if let (Some(username), Some(password)) =
            (config.smtp_username.clone(), config.smtp_password.clone())
        {
            builder = builder.credentials(Credentials::new(username, password));
        }
        Ok(Self {
            transport: builder.build(),
            from,
            subject: Arc::from(config.subject.clone()),
            templates: TemplateService::new()?,
            max_concurrent: config.max_concurrent,
            timeout,
        })
    }

    /// Проверяет доступность SMTP-сервера без отправки письма.
    pub(crate) async fn test_connection(&self) -> Result<(), MailError> {
        let connected = tokio::time::timeout(self.timeout, self.transport.test_connection())
            .await
            .map_err(|_| MailError::Timeout)?
            .map_err(|source| MailError::ConnectionFailed { source })?;
        if connected {
            Ok(())
        } else {
            Err(MailError::AuthenticationFailed)
        }
    }

    /// Рендерит письмо credentials через загруженные шаблоны.
    pub(crate) fn render_credentials(
        &self,
        data: CredentialTemplateData<'_>,
    ) -> Result<RenderedMail, MailError> {
        self.templates.render(data)
    }

    /// Отправляет одно заранее подготовленное письмо.
    pub(crate) async fn send(&self, mail: PreparedMail) -> Result<(), MailError> {
        let recipient = mail
            .recipient
            .parse()
            .map_err(|source| MailError::InvalidRecipient {
                email: mail.recipient.clone(),
                source,
            })?;
        let message = Message::builder()
            .from(self.from.clone())
            .to(recipient)
            .subject(self.subject.as_ref())
            .header(ContentType::TEXT_PLAIN)
            .multipart(MultiPart::alternative_plain_html(
                mail.plain_text_body,
                mail.html_body,
            ))
            .map_err(|source| MailError::MessageBuild { source })?;
        self.transport
            .send(message)
            .await
            .map(|_| ())
            .map_err(Self::classify_smtp_error)
    }

    /// Классифицирует SMTP-ошибку для retry-политики и итогового статуса.
    fn classify_smtp_error(source: lettre::transport::smtp::Error) -> MailError {
        if source.is_timeout() {
            MailError::Timeout
        } else if source.is_transient() {
            MailError::TemporaryFailure { source }
        } else if source.is_permanent() {
            MailError::PermanentFailure { source }
        } else {
            MailError::ConnectionFailed { source }
        }
    }

    /// Выполняет отправку с тремя повторами для временных ошибок.
    async fn send_with_retry(&self, mail: PreparedMail) -> Result<(), MailError> {
        let mut attempt = 0;
        loop {
            match self.send(mail.clone()).await {
                Ok(()) => return Ok(()),
                Err(error) if is_retryable(&error) && attempt < 3 => {
                    let delay = [1, 3, 10][attempt];
                    attempt += 1;
                    tracing::warn!(attempt, delay_seconds = delay, error = ?error, "повторная отправка письма после временной ошибки");
                    tokio::time::sleep(Duration::from_secs(delay)).await;
                }
                Err(error) => return Err(error),
            }
        }
    }

    /// Отправляет пакет писем с ограничением количества параллельных операций.
    pub(crate) async fn send_batch(&self, mails: Vec<PreparedMail>) -> Vec<MailDeliveryResult> {
        let limit = self.max_concurrent.max(1);
        stream::iter(mails.into_iter().map(|mail| async move {
            let row_id = mail.row_id.clone();
            let email = mail.recipient.clone();
            match self.send_with_retry(mail).await {
                Ok(()) => MailDeliveryResult {
                    row_id,
                    email,
                    status: MailDeliveryStatus::AcceptedBySmtp,
                },
                Err(error) => (row_id, email, error).into(),
            }
        }))
        .buffer_unordered(limit)
        .collect()
        .await
    }
}

/// Возвращает, нужно ли повторять конкретную SMTP-операцию.
fn is_retryable(error: &MailError) -> bool {
    matches!(
        error,
        MailError::ConnectionFailed { .. }
            | MailError::TemporaryFailure { .. }
            | MailError::Timeout
    )
}
