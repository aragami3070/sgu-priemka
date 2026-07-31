use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    entities::auth::{LdapCredentials, LdapIdentity, Session, SessionId},
    errors::AppError,
};

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

    /// Создаёт локальную сессию и отдельный UUID хранилища результатов.
    pub(crate) async fn create(
        &self,
        _identity: LdapIdentity,
    ) -> Result<(SessionId, Session), AppError> {
        todo!("create an opaque local session and storage identifier")
    }

    /// Возвращает неистёкшую сессию по непрозрачному идентификатору.
    pub(crate) async fn get(&self, _id: &SessionId) -> Result<Session, AppError> {
        todo!("load and validate a local session")
    }

    /// Удаляет и возвращает локальную сессию.
    pub(crate) async fn remove(&self, _id: &SessionId) -> Option<Session> {
        todo!("remove a local session")
    }

    /// Удаляет сессии с истёкшим локальным сроком действия.
    pub(crate) async fn cleanup_expired(&self) {
        todo!("remove expired local sessions")
    }
}
