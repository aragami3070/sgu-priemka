//! Переиспользуемый SMTP-сервис для отправки credentials студентов.

mod batch;
mod smtp;
mod template;

pub(crate) use batch::{MailDeliveryStatus, PreparedMail};
pub(crate) use smtp::MailService;
pub(crate) use template::CredentialTemplateData;
