//! Переиспользуемый SMTP-сервис для отправки credentials студентов.

mod batch;
mod smtp;
mod template;

pub(crate) use batch::{MailDeliveryResult, MailDeliveryStatus, PreparedMail};
pub(crate) use smtp::MailService;
pub(crate) use template::{CredentialTemplateData, RenderedMail};
