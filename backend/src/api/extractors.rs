use std::sync::Arc;

use axum::{extract::FromRequestParts, http::request::Parts};

use crate::{
    api::cookies,
    entities::auth::{LdapCredentials, SessionId},
    errors::AppError,
    state::AppState,
};

/// Данные аутентифицированного пользователя для защищённых обработчиков.
#[derive(Clone)]
pub(super) struct AuthenticatedUser {
    /// Идентификатор локальной сессии, прочитанный из cookie и проверенный в хранилище.
    pub(super) session_id: SessionId,
    /// Каноническое имя пользователя из `sAMAccountName`.
    pub(super) username: String,
    /// Credentials текущей сессии для LDAP-операций от имени вошедшего пользователя.
    pub(super) ldap_credentials: Arc<LdapCredentials>,
}

impl FromRequestParts<AppState> for AuthenticatedUser {
    type Rejection = AppError;

    /// Находит по cookie действующую серверную сессию до запуска обработчика.
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let session_id = cookies::session_id_from_headers(&parts.headers)?;
        let session = state.sessions.get(&session_id).await?;
        tracing::info!(
            username = %session.username,
            ldap_identifier = session.ldap_credentials.identifier(),
            "authenticated user extraction completed"
        );

        Ok(Self {
            session_id,
            username: session.username,
            ldap_credentials: session.ldap_credentials,
        })
    }
}
