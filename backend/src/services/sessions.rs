use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    entities::auth::{LdapCredentials, LdapIdentity, Session, SessionId},
    errors::AppError,
};
use tokio::sync::RwLock;

/// Хранилище локальных сессий в памяти.
///
/// Cookie содержит только `SessionId`; LDAP credentials остаются в памяти backend.
pub(crate) struct SessionService {
    /// Сессии индексируются только по непрозрачному значению из cookie.
    store: RwLock<HashMap<SessionId, Session>>,
    /// Единый срок действия новых сессий из конфигурации приложения.
    ttl: Duration,
}

impl SessionService {
    /// Создаёт пустое хранилище сессий в памяти.
    pub(crate) fn new(ttl: Duration) -> Self {
        Self {
            store: RwLock::new(HashMap::new()),
            ttl,
        }
    }

    /// Создаёт локальную сессию с credentials пользователя.
    pub(crate) async fn create(
        &self,
        identity: LdapIdentity,
        ldap_credentials: LdapCredentials,
    ) -> Result<(SessionId, Session), AppError> {
        let expires_at = Instant::now()
            .checked_add(self.ttl)
            .ok_or(AppError::Internal)?;
        let session = Session {
            username: identity.username,
            ldap_credentials: Arc::new(ldap_credentials),
            expires_at,
        };

        let mut store = self.store.write().await;
        let id = loop {
            let candidate = SessionId::new();
            if !store.contains_key(&candidate) {
                break candidate;
            }
        };

        store.insert(id.clone(), session.clone());
        tracing::info!(
            username = %session.username,
            active_sessions = store.len(),
            "local session created and stored"
        );

        Ok((id, session))
    }

    /// Возвращает неистёкшую сессию по непрозрачному идентификатору.
    pub(crate) async fn get(&self, id: &SessionId) -> Result<Session, AppError> {
        let mut store = self.store.write().await;
        let Some(session) = store.get(id) else {
            tracing::info!(active_sessions = store.len(), "local session was not found");
            return Err(AppError::Unauthorized);
        };

        if session.expires_at <= Instant::now() {
            let username = session.username.clone();
            store.remove(id);
            tracing::info!(
                %username,
                active_sessions = store.len(),
                "expired local session removed during lookup"
            );
            return Err(AppError::Unauthorized);
        }

        tracing::info!(
            username = %session.username,
            "valid local session found"
        );
        Ok(session.clone())
    }

    /// Удаляет и возвращает локальную сессию.
    pub(crate) async fn remove(&self, id: &SessionId) -> Option<Session> {
        let mut store = self.store.write().await;
        let removed = store.remove(id);
        match &removed {
            Some(session) => tracing::info!(
                username = %session.username,
                active_sessions = store.len(),
                "local session removed"
            ),
            None => tracing::info!(
                active_sessions = store.len(),
                "local session to remove was not found"
            ),
        }
        removed
    }

    /// Удаляет сессии с истёкшим локальным сроком действия.
    pub(crate) async fn cleanup_expired(&self) {
        tracing::info!("starting expired session cleanup");
        let now = Instant::now();
        let mut store = self.store.write().await;
        let before = store.len();
        store.retain(|_, session| session.expires_at > now);
        tracing::info!(
            sessions_before = before,
            sessions_after = store.len(),
            removed_sessions = before - store.len(),
            "expired session cleanup completed"
        );
    }
}
