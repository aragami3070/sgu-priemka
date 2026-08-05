use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use thiserror::Error;

use super::{KerberosError, LdapError, ResultError};

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
    /// Запрошенный сохранённый результат отсутствует.
    #[error("result not found")]
    NotFound,
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
        let (status, message) = match self {
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "authentication required".to_owned(),
            ),
            Self::Forbidden => (StatusCode::FORBIDDEN, "access denied".to_owned()),
            Self::ImportBusy => (
                StatusCode::CONFLICT,
                "another import is already running".to_owned(),
            ),
            Self::InvalidUpload(message) => (StatusCode::BAD_REQUEST, message),
            Self::Validation(message) => (StatusCode::UNPROCESSABLE_ENTITY, message),
            Self::NotFound => (StatusCode::NOT_FOUND, "result not found".to_owned()),
            Self::LdapUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "LDAP is unavailable".to_owned(),
            ),
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal server error".to_owned(),
            ),
        };
        (status, Json(message)).into_response()
    }
}

impl From<LdapError> for AppError {
    fn from(error: LdapError) -> Self {
        tracing::warn!(error = ?error, "LDAP request failed");
        match error {
            LdapError::InvalidCredentials => Self::Unauthorized,
            LdapError::Forbidden => Self::Forbidden,
            LdapError::Connect { .. }
            | LdapError::Kerberos { .. }
            | LdapError::AuthenticationTransport { .. }
            | LdapError::AuthenticationRejected { .. }
            | LdapError::Search { .. }
            | LdapError::Operation { .. } => Self::LdapUnavailable,
            LdapError::MissingAttribute { .. } | LdapError::UnexpectedSearchResult { .. } => {
                Self::Internal
            }
        }
    }
}

impl From<KerberosError> for AppError {
    fn from(error: KerberosError) -> Self {
        tracing::warn!(error = ?error, "Kerberos request failed");
        match error {
            KerberosError::InvalidCredentials => Self::Unauthorized,
            KerberosError::CacheIo { .. }
            | KerberosError::Library { .. }
            | KerberosError::Gssapi { .. }
            | KerberosError::BlockingTask { .. }
            | KerberosError::InvalidExpiration
            | KerberosError::InteriorNul { .. }
            | KerberosError::CacheAlreadyExists(_) => Self::LdapUnavailable,
        }
    }
}

impl From<ResultError> for AppError {
    fn from(error: ResultError) -> Self {
        tracing::error!(%error, "mapping result service error to application error");
        Self::Internal
    }
}
