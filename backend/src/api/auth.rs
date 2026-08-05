use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use axum_extra::extract::CookieJar;
use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use zeroize::Zeroizing;

use crate::{
    api::{cookies, extractors::AuthenticatedUser},
    entities::auth::SessionId,
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

/// Учётные данные для получения персонального Kerberos TGT.
#[derive(Deserialize)]
struct LoginRequest {
    /// Короткий идентификатор, из которого backend сформирует `<identifier>@REALM`.
    identifier: String,
    /// Пароль, очищаемый сразу после ответа KDC.
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
    Json(WhoAmIResponse {
        username: user.username,
    })
}

/// Получает TGT, выполняет GSSAPI-аутентификацию, проверяет admin group и создаёт сессию.
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
    let password = Zeroizing::new(password);
    let session_id = SessionId::new();
    let mut credentials = state
        .kerberos
        .acquire_tgt(identifier, password, &session_id)
        .await?;
    let identity = match state.ldap.authenticate(&credentials).await {
        Ok(identity) => identity,
        Err(error) => {
            state.kerberos.destroy_cache(&credentials).await;
            return Err(error.into());
        }
    };

    let username = identity.username.clone();
    credentials.set_identifier(username.clone());
    let previous_session_id = cookies::session_id_from_jar(&jar).ok();
    let session = match state
        .sessions
        .create(session_id.clone(), identity, credentials.clone())
        .await
    {
        Ok(session) => session,
        Err(error) => {
            state.kerberos.destroy_cache(&credentials).await;
            return Err(error);
        }
    };
    let cookie_ttl = session
        .expires_at
        .duration_since(std::time::SystemTime::now())
        .map_err(|_| AppError::Unauthorized)?;
    let expires_at = session_expiration(session.expires_at)?;
    let jar = match cookies::add_session_cookie(
        jar,
        &session_id,
        cookie_ttl,
        state.config.cookie_secure,
    ) {
        Ok(jar) => jar,
        Err(error) => {
            tracing::warn!(%username, %error, "failed to build session cookie; removing newly created session");
            state.sessions.remove(&session_id).await;
            return Err(error);
        }
    };
    if let Some(previous_session_id) = previous_session_id {
        state.sessions.remove(&previous_session_id).await;
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
    Ok((jar, StatusCode::NO_CONTENT))
}

/// Возвращает абсолютный UTC-срок сессии в формате RFC 3339 для frontend.
fn session_expiration(expires_at: std::time::SystemTime) -> Result<String, AppError> {
    let expires_at = OffsetDateTime::from(expires_at);
    let formatted = expires_at
        .format(&Rfc3339)
        .map_err(|_| AppError::Internal)?;
    Ok(formatted)
}
