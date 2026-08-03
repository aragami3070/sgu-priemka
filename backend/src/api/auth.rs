use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use axum_extra::extract::CookieJar;
use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    api::{cookies, extractors::AuthenticatedUser},
    entities::auth::LdapCredentials,
    errors::AppError,
    state::AppState,
};

/// Объявляет маршруты аутентификации без подключения состояния приложения.
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/whoami", get(whoami))
}

/// Учётные данные для прямого пользовательского LDAP bind.
#[derive(Deserialize)]
struct LoginRequest {
    /// Короткий идентификатор, из которого backend сформирует `DOMAIN\\identifier`.
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

/// Минимальные данные действующей локальной сессии для восстановления frontend-состояния.
#[derive(Debug, Serialize)]
struct WhoAmIResponse {
    /// Каноническое имя пользователя из `sAMAccountName`.
    username: String,
}

/// Проверяет локальную cookie-сессию и возвращает имя вошедшего пользователя.
async fn whoami(user: AuthenticatedUser) -> Json<WhoAmIResponse> {
    tracing::info!(username = %user.username, "current local session returned to frontend");
    Json(WhoAmIResponse {
        username: user.username,
    })
}

/// Выполняет LDAP bind, проверяет admin group dn и создаёт локальную сессию.
async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(request): Json<LoginRequest>,
) -> Result<(CookieJar, Json<LoginResponse>), AppError> {
    let LoginRequest {
        identifier,
        password,
    } = request;
    let identifier = identifier.trim().to_owned();
    let identity = state.ldap.authenticate(&identifier, &password).await?;

    let username = identity.username.clone();
    let credentials = LdapCredentials::new(username.clone(), password);
    let previous_session_id = cookies::session_id_from_jar(&jar).ok();
    let expires_at = session_expiration(state.config.session_ttl)?;
    let (session_id, _) = state.sessions.create(identity, credentials).await?;
    tracing::info!(%username, "server-side login session created");

    let jar = match cookies::add_session_cookie(
        jar,
        &session_id,
        state.config.session_ttl,
        state.config.cookie_secure,
    ) {
        Ok(jar) => jar,
        Err(error) => {
            tracing::warn!(%username, %error, "failed to build session cookie; removing newly created session");
            state.sessions.remove(&session_id).await;
            return Err(error);
        }
    };
    tracing::info!(%username, "session cookie added to login response");

    if let Some(previous_session_id) = previous_session_id {
        state.sessions.remove(&previous_session_id).await;
        tracing::info!(%username, "previous session removal completed");
    }

    tracing::info!(%username, "user logged in");

    Ok((
        jar,
        Json(LoginResponse {
            username,
            expires_at,
        }),
    ))
}

/// Удаляет cookie и соответствующую локальную сессию.
async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<(CookieJar, StatusCode), AppError> {
    if let Ok(session_id) = cookies::session_id_from_jar(&jar)
        && let Some(session) = state.sessions.remove(&session_id).await
    {
        tracing::info!(username = %session.username, "user logged out");
    }

    let jar = cookies::remove_session_cookie(jar, state.config.cookie_secure);
    tracing::info!("logout request completed");
    Ok((jar, StatusCode::NO_CONTENT))
}

/// Возвращает абсолютный UTC-срок сессии в формате RFC 3339 для frontend.
fn session_expiration(ttl: std::time::Duration) -> Result<String, AppError> {
    let ttl = time::Duration::try_from(ttl).map_err(|_| AppError::Internal)?;
    let expires_at = OffsetDateTime::now_utc()
        .checked_add(ttl)
        .ok_or(AppError::Internal)?;

    let formatted = expires_at
        .format(&Rfc3339)
        .map_err(|_| AppError::Internal)?;
    Ok(formatted)
}
