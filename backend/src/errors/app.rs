use axum::response::{IntoResponse, Response};
use thiserror::Error;

/// Ошибки, которые прикладные сервисы могут передать в HTTP-слой.
#[derive(Debug, Error)]
pub(crate) enum AppError {
    /// В запросе отсутствует действующая локальная сессия.
    #[error("authentication required")]
    Unauthorized,
    /// Аутентифицированному пользователю запрещена операция.
    #[error("access denied")]
    Forbidden,
    /// Глобальная блокировка уже занята другим импортом в LDAP.
    #[error("another import is already running")]
    ImportBusy,
    /// Загруженный multipart-файл отсутствует или имеет неверную структуру.
    #[error("invalid upload: {0}")]
    InvalidUpload(String),
    /// Входные данные не прошли бизнес-валидацию до начала записи в LDAP.
    #[error("validation failed: {0}")]
    Validation(String),
    /// LDAP-сервер не смог выполнить запрошенную операцию.
    #[error("LDAP is unavailable")]
    LdapUnavailable,
    /// Внутренняя ошибка, детали которой нельзя раскрывать клиенту.
    #[error("internal server error")]
    Internal,
}

impl IntoResponse for AppError {
    /// Преобразует внутренние ошибки в стабильный публичный формат HTTP-ответа.
    fn into_response(self) -> Response {
        todo!("map application errors to the public API error envelope")
    }
}
