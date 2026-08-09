use std::{collections::HashMap, sync::Arc, time::Duration};

use futures_util::{StreamExt, stream};
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Mailbox, MultiPart},
    transport::smtp::authentication::Credentials,
};
use tokio::sync::RwLock;

use crate::{
    config::{MailConfig, SmtpSecurity},
    errors::MailError,
};

use super::{
    batch::{MailDeliveryResult, MailDeliveryStatus, PreparedMail},
    template::{CredentialTemplateData, RenderedMail, TemplateService},
};

/// Состояние рассылки для конкретного CSV-результата.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MailBatchStatus {
    /// Рассылка выполняется.
    Running,
    /// Все письма приняты SMTP.
    Completed,
    /// Часть писем не доставлена.
    PartiallyFailed,
    /// Рассылка полностью провалилась.
    Failed,
}

/// Переиспользуемый SMTP-сервис с общим connection pool.
#[derive(Clone)]
pub(crate) struct MailService {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
    subject: Arc<str>,
    templates: TemplateService,
    max_concurrent: usize,
    timeout: Duration,
    /// Трекер статуса рассылки по ключу `"{owner}/{filename}"`.
    deliveries: Arc<RwLock<HashMap<String, MailBatchStatus>>>,
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
        match (config.smtp_username.as_ref(), config.smtp_password.as_ref()) {
            (Some(username), Some(password)) => {
                builder = builder.credentials(Credentials::new(username.clone(), password.clone()));
            }
            // Валидно для relay, который доверяет IP приложения.
            (None, None) => {}
            // Ошибка конфигурации: задана только часть credentials.
            _ => {
                return Err(MailError::InvalidConfig(
                    "SMTP username and password must be configured together".into(),
                ));
            }
        }
        Ok(Self {
            transport: builder.build(),
            from,
            subject: Arc::from(config.subject.clone()),
            templates: TemplateService::new()?,
            max_concurrent: config.max_concurrent,
            timeout,
            deliveries: Arc::new(RwLock::new(HashMap::new())),
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
            Err(MailError::ConnectionTestFailed)
        }
    }

    /// Рендерит письмо credentials через загруженные шаблоны.
    pub(crate) fn render_credentials(
        &self,
        data: CredentialTemplateData<'_>,
    ) -> Result<RenderedMail, MailError> {
        self.templates.render(data)
    }

    /// Атомарно пытается перевести рассылку в `Running`.
    ///
    /// Допускает повторный запуск только если предыдущая завершилась ошибкой.
    pub(crate) async fn try_start_delivery(&self, key: &str) -> Result<(), MailError> {
        let mut deliveries = self.deliveries.write().await;
        match deliveries.get(key) {
            None | Some(MailBatchStatus::Failed) | Some(MailBatchStatus::PartiallyFailed) => {
                deliveries.insert(key.to_owned(), MailBatchStatus::Running);
                Ok(())
            }
            Some(MailBatchStatus::Running) => Err(MailError::InvalidConfig(
                "mail delivery is already running for this result".into(),
            )),
            Some(MailBatchStatus::Completed) => Err(MailError::InvalidConfig(
                "mail delivery already completed for this result".into(),
            )),
        }
    }

    /// Обновляет состояние рассылки после завершения.
    pub(crate) async fn finish_delivery(&self, key: &str, status: MailBatchStatus) {
        self.deliveries.write().await.insert(key.to_owned(), status);
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
            // Проверяем TLS-ошибку по Debug-представлению, потому что lettre
            // не предоставляет отдельного метода для TLS classification.
            let debug = format!("{source:?}");
            if debug.contains("Tls") || debug.contains("tls") || debug.contains("certificate") {
                MailError::TlsFailure { source }
            } else {
                MailError::ConnectionFailed { source }
            }
        }
    }

    /// Выполняет отправку с тремя повторами для временных ошибок.
    async fn send_with_retry(&self, mail: PreparedMail) -> Result<(), MailError> {
        let mut attempt = 0;
        loop {
            match self.send(mail.clone()).await {
                Ok(()) => return Ok(()),
                Err(error) if error.is_retryable() && attempt < 3 => {
                    let delay = [1, 3, 10][attempt];
                    attempt += 1;
                    tracing::warn!(attempt, delay_seconds = delay, error = ?error, "повторная отправка письма после временной ошибки");
                    tokio::time::sleep(Duration::from_secs(delay)).await;
                }
                Err(error) => return Err(error),
            }
        }
    }

    /// Возвращает stream результатов отправки с ограничением параллелизма.
    ///
    /// Каждый результат возвращается немедленно после завершения SMTP-операции,
    /// позволяя orchestration-слою публиковать WebSocket progress в реальном времени.
    pub(crate) fn send_batch_stream(
        &self,
        mails: Vec<PreparedMail>,
    ) -> impl futures_util::Stream<Item = MailDeliveryResult> + Send + 'static {
        let limit = self.max_concurrent.max(1);
        let service = self.clone();
        stream::iter(mails.into_iter().map(move |mail| {
            let svc = service.clone();
            async move {
                let row_id = mail.row_id.clone();
                let email = mail.recipient.clone();
                match svc.send_with_retry(mail).await {
                    Ok(()) => MailDeliveryResult {
                        row_id,
                        email,
                        status: MailDeliveryStatus::AcceptedBySmtp,
                    },
                    Err(error) => (row_id, email, error).into(),
                }
            }
        }))
        .buffer_unordered(limit)
    }
}
