use std::time::Duration;

use axum::http::HeaderMap;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};

use crate::{entities::auth::SessionId, errors::AppError};

const SESSION_COOKIE_NAME: &str = "session";
const SESSION_COOKIE_PATH: &str = "/";

/// Добавляет в ответ cookie, содержащую только непрозрачный идентификатор сессии.
pub(super) fn add_session_cookie(
    jar: CookieJar,
    session_id: &SessionId,
    ttl: Duration,
    secure: bool,
) -> Result<CookieJar, AppError> {
    let max_age = time::Duration::try_from(ttl).map_err(|_| AppError::Internal)?;
    let cookie = Cookie::build((SESSION_COOKIE_NAME, session_id.to_string()))
        .http_only(true)
        .secure(secure)
        .same_site(SameSite::Lax)
        .path(SESSION_COOKIE_PATH)
        .max_age(max_age)
        .build();

    tracing::info!(
        cookie_name = SESSION_COOKIE_NAME,
        "session cookie added to response jar"
    );
    Ok(jar.add(cookie))
}

/// Удаляет session cookie браузера с теми же path и security-атрибутами.
pub(super) fn remove_session_cookie(jar: CookieJar, secure: bool) -> CookieJar {
    let cookie = Cookie::build((SESSION_COOKIE_NAME, ""))
        .http_only(true)
        .secure(secure)
        .same_site(SameSite::Lax)
        .path(SESSION_COOKIE_PATH)
        .build();

    tracing::info!(
        cookie_name = SESSION_COOKIE_NAME,
        "session cookie removal added to response jar"
    );
    jar.remove(cookie)
}

/// Читает UUID сессии из входящей cookie без проверки её существования в хранилище.
pub(super) fn session_id_from_headers(headers: &HeaderMap) -> Result<SessionId, AppError> {
    tracing::info!(
        cookie_header_count = headers.get_all(axum::http::header::COOKIE).iter().count(),
        "reading session cookie from request headers"
    );
    session_id_from_jar(&CookieJar::from_headers(headers))
}

/// Читает UUID сессии из уже извлечённого cookie jar.
pub(super) fn session_id_from_jar(jar: &CookieJar) -> Result<SessionId, AppError> {
    let Some(cookie) = jar.get(SESSION_COOKIE_NAME) else {
        tracing::info!(
            cookie_name = SESSION_COOKIE_NAME,
            "session cookie is missing"
        );
        return Err(AppError::Unauthorized);
    };
    tracing::info!(cookie_name = SESSION_COOKIE_NAME, "session cookie found");

    let session_id = cookie.value().parse().map_err(|_| {
        tracing::info!(
            cookie_name = SESSION_COOKIE_NAME,
            "session cookie does not contain a valid UUID"
        );
        AppError::Unauthorized
    })?;

    tracing::info!(
        cookie_name = SESSION_COOKIE_NAME,
        "session cookie UUID parsed successfully"
    );
    Ok(session_id)
}
