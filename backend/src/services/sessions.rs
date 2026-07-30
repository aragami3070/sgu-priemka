use crate::{
    entities::auth::{LdapIdentity, Session, SessionId},
    errors::AppError,
};

/// Хранилище локальных сессий в памяти.
///
/// Хранилище связывает cookie только с локальными данными и не содержит LDAP credentials.
#[derive(Default)]
pub(crate) struct SessionService;

impl SessionService {
    /// Создаёт пустое хранилище сессий в памяти.
    pub(crate) fn new() -> Self {
        todo!("initialize the in-memory session store")
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
