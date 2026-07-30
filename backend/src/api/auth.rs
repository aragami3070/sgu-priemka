use axum::{Json, Router, extract::State, http::StatusCode, routing::post};
use axum_extra::extract::CookieJar;
use serde::{Deserialize, Serialize};

use crate::{api::extractors::AuthenticatedUser, errors::AppError, state::AppState};

/// Объявляет маршруты аутентификации без подключения состояния приложения.
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
}

/// Учётные данные для прямого пользовательского LDAP bind.
#[derive(Debug, Deserialize)]
struct LoginRequest {
    /// Короткий идентификатор, из которого backend сформирует Bind DN.
    identifier: String,
    /// Пароль, используемый только во время пользовательского LDAP bind.
    password: String,
}

/// Публичные данные пользователя и срок сессии после успешного входа.
#[derive(Debug, Serialize)]
struct LoginResponse {
    /// Каноническое имя пользователя из `sAMAccountName`.
    username: String,
    /// Срок локальной сессии в формате для фронтенда.
    expires_at: String,
}

/// Выполняет LDAP bind, проверяет `csit_admins` и создаёт локальную сессию.
async fn login(
    State(_state): State<AppState>,
    _jar: CookieJar,
    Json(_request): Json<LoginRequest>,
) -> Result<(CookieJar, Json<LoginResponse>), AppError> {
    todo!("authenticate through LDAP and create a local session")
}

/// Удаляет cookie и соответствующую локальную сессию.
async fn logout(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    _jar: CookieJar,
) -> Result<(CookieJar, StatusCode), AppError> {
    todo!("invalidate the local session and remove its cookie")
}
