use std::sync::Arc;

use handlebars::Handlebars;
use serde_json::json;

use crate::errors::MailError;

const HTML_TEMPLATE: &str = include_str!("../../../templates/credentials.html");
const TEXT_TEMPLATE: &str = include_str!("../../../templates/credentials.txt");

/// Данные студента, доступные шаблону письма.
pub(crate) struct CredentialTemplateData<'a> {
    /// Логин новой учётной записи.
    pub(crate) login: &'a str,
    /// Временный пароль новой учётной записи.
    pub(crate) temporary_password: &'a str,
}

/// Отрендеренные HTML- и plain-text версии письма.
pub(crate) struct RenderedMail {
    /// HTML-содержимое письма.
    pub(crate) html: String,
    /// Текстовая альтернатива письма.
    pub(crate) plain_text: String,
}

/// Загруженные при старте сервера шаблоны credentials.
#[derive(Clone)]
pub(crate) struct TemplateService {
    html: Arc<Handlebars<'static>>,
    text: Arc<Handlebars<'static>>,
}

impl TemplateService {
    /// Загружает и компилирует встроенные шаблоны один раз.
    pub(crate) fn new() -> Result<Self, MailError> {
        let mut html = Handlebars::new();
        html.register_template_string("credentials", HTML_TEMPLATE)
            .map_err(|error| MailError::TemplateRender {
                reason: error.to_string(),
            })?;
        let mut text = Handlebars::new();
        text.register_escape_fn(handlebars::no_escape);
        text.register_template_string("credentials", TEXT_TEMPLATE)
            .map_err(|error| MailError::TemplateRender {
                reason: error.to_string(),
            })?;
        Ok(Self {
            html: Arc::new(html),
            text: Arc::new(text),
        })
    }

    /// Рендерит обе MIME-версии письма без повторного чтения шаблонов с диска.
    pub(crate) fn render(
        &self,
        data: CredentialTemplateData<'_>,
    ) -> Result<RenderedMail, MailError> {
        let values = json!({
            "login": data.login,
            "temporary_password": data.temporary_password,
        });
        let html = self.html.render("credentials", &values).map_err(|error| {
            MailError::TemplateRender {
                reason: error.to_string(),
            }
        })?;
        let plain_text = self.text.render("credentials", &values).map_err(|error| {
            MailError::TemplateRender {
                reason: error.to_string(),
            }
        })?;
        Ok(RenderedMail { html, plain_text })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_html_with_escaping_and_plain_text_alternative() {
        let service = TemplateService::new().expect("шаблоны должны загрузиться");
        let rendered = service
            .render(CredentialTemplateData {
                login: "<ivanovii>",
                temporary_password: "secret",
            })
            .expect("шаблоны должны отрендериться");

        assert!(rendered.html.contains("&lt;ivanovii&gt;"));
        assert!(rendered.plain_text.contains("<ivanovii>"));
        assert!(rendered.html.contains("secret"));
        assert!(rendered.plain_text.contains("secret"));
    }
}
