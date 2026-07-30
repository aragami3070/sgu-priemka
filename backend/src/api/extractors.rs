use axum::{extract::FromRequestParts, http::request::Parts};
use uuid::Uuid;

use crate::{errors::AppError, state::AppState};

/// Данные аутентифицированного пользователя для защищённых обработчиков.
#[derive(Clone, Debug)]
pub(super) struct AuthenticatedUser {
    /// Каноническое имя пользователя из `sAMAccountName`.
    pub(super) username: String,
    /// Непрозрачный идентификатор группы файлов этой сессии.
    pub(super) storage_id: Uuid,
}

impl FromRequestParts<AppState> for AuthenticatedUser {
    type Rejection = AppError;

    /// Находит по cookie действующую серверную сессию до запуска обработчика.
    async fn from_request_parts(
        _parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        todo!("read the session cookie and validate the local session")
    }
}
