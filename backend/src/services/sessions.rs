use std::{
    collections::HashMap,
    fs::OpenOptions,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{
    entities::auth::{LdapCredentials, LdapIdentity, Session, SessionId},
    errors::AppError,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};

/// Хранилище локальных сессий в памяти с debug-only восстановлением из файла.
///
/// Cookie содержит только `SessionId`. Release-сборка держит LDAP credentials
/// только в памяти; debug-сборка сохраняет их локально между перезапусками.
pub(crate) struct SessionService {
    /// Сессии индексируются только по непрозрачному значению из cookie.
    store: RwLock<HashMap<SessionId, Session>>,
    /// Единый срок действия новых сессий из конфигурации приложения.
    ttl: Duration,
    /// Debug-only файл, позволяющий пережить перезапуск backend во время разработки.
    persistence_path: Option<PathBuf>,
    /// Не допускает гонки нескольких атомарных записей одного snapshot-файла.
    persistence_lock: Mutex<()>,
}

#[derive(Serialize, Deserialize)]
struct PersistedSession {
    id: String,
    username: String,
    identifier: String,
    password: String,
    expires_at_unix_ms: u64,
}

impl SessionService {
    /// Создаёт пустое хранилище сессий в памяти.
    pub(crate) fn new(ttl: Duration) -> Self {
        let persistence_path = cfg!(all(debug_assertions, not(test)))
            .then(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".dev-sessions.json"));
        Self::with_persistence(ttl, persistence_path)
    }

    fn with_persistence(ttl: Duration, persistence_path: Option<PathBuf>) -> Self {
        let store = persistence_path
            .as_deref()
            .map(load_sessions)
            .unwrap_or_default();
        if let Some(path) = &persistence_path {
            tracing::info!(
                path = %path.display(),
                restored_sessions = store.len(),
                "debug session persistence enabled"
            );
        }

        Self {
            store: RwLock::new(store),
            ttl,
            persistence_path,
            persistence_lock: Mutex::new(()),
        }
    }

    /// Создаёт локальную сессию с credentials пользователя.
    pub(crate) async fn create(
        &self,
        identity: LdapIdentity,
        ldap_credentials: LdapCredentials,
    ) -> Result<(SessionId, Session), AppError> {
        let expires_at = SystemTime::now()
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
        drop(store);
        self.persist().await;

        Ok((id, session))
    }

    /// Возвращает неистёкшую сессию по непрозрачному идентификатору.
    pub(crate) async fn get(&self, id: &SessionId) -> Result<Session, AppError> {
        let mut store = self.store.write().await;
        let Some(session) = store.get(id) else {
            tracing::info!(active_sessions = store.len(), "local session was not found");
            return Err(AppError::Unauthorized);
        };

        if session.expires_at <= SystemTime::now() {
            let username = session.username.clone();
            store.remove(id);
            tracing::info!(
                %username,
                active_sessions = store.len(),
                "expired local session removed during lookup"
            );
            drop(store);
            self.persist().await;
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
        drop(store);
        if removed.is_some() {
            self.persist().await;
        }
        removed
    }

    /// Удаляет сессии с истёкшим локальным сроком действия.
    pub(crate) async fn cleanup_expired(&self) {
        tracing::info!("starting expired session cleanup");
        let now = SystemTime::now();
        let mut store = self.store.write().await;
        let before = store.len();
        store.retain(|_, session| session.expires_at > now);
        tracing::info!(
            sessions_before = before,
            sessions_after = store.len(),
            removed_sessions = before - store.len(),
            "expired session cleanup completed"
        );
        let changed = before != store.len();
        drop(store);
        if changed {
            self.persist().await;
        }
    }

    async fn persist(&self) {
        let Some(path) = self.persistence_path.clone() else {
            return;
        };
        let _guard = self.persistence_lock.lock().await;
        let snapshot = {
            let store = self.store.read().await;
            persisted_snapshot(&store)
        };
        let persisted_sessions = snapshot.len();

        let result = tokio::task::spawn_blocking(move || write_sessions(&path, &snapshot)).await;
        match result {
            Ok(Ok(())) => tracing::info!(persisted_sessions, "debug sessions persisted"),
            Ok(Err(error)) => tracing::warn!(%error, "failed to persist debug sessions"),
            Err(error) => tracing::warn!(%error, "debug session persistence task failed"),
        }
    }
}

fn persisted_snapshot(store: &HashMap<SessionId, Session>) -> Vec<PersistedSession> {
    store
        .iter()
        .filter_map(|(id, session)| {
            let expires_at_unix_ms = session
                .expires_at
                .duration_since(UNIX_EPOCH)
                .ok()?
                .as_millis()
                .try_into()
                .ok()?;
            Some(PersistedSession {
                id: id.to_string(),
                username: session.username.clone(),
                identifier: session.ldap_credentials.identifier().to_owned(),
                password: session.ldap_credentials.password().to_owned(),
                expires_at_unix_ms,
            })
        })
        .collect()
}

fn load_sessions(path: &Path) -> HashMap<SessionId, Session> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return HashMap::new(),
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "failed to read debug sessions");
            return HashMap::new();
        }
    };
    let persisted: Vec<PersistedSession> = match serde_json::from_slice(&bytes) {
        Ok(persisted) => persisted,
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "failed to parse debug sessions");
            return HashMap::new();
        }
    };

    let now = SystemTime::now();
    persisted
        .into_iter()
        .filter_map(|session| {
            let id = session.id.parse::<SessionId>().ok()?;
            let expires_at =
                UNIX_EPOCH.checked_add(Duration::from_millis(session.expires_at_unix_ms))?;
            (expires_at > now).then(|| {
                (
                    id,
                    Session {
                        username: session.username,
                        ldap_credentials: Arc::new(LdapCredentials::new(
                            session.identifier,
                            session.password,
                        )),
                        expires_at,
                    },
                )
            })
        })
        .collect()
}

fn write_sessions(path: &Path, sessions: &[PersistedSession]) -> io::Result<()> {
    let bytes = serde_json::to_vec(sessions).map_err(io::Error::other)?;
    let temporary_path = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let result = (|| {
        let mut file = options.open(&temporary_path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        std::fs::rename(&temporary_path, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary_path);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestFile(PathBuf);

    impl TestFile {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!(
                "sgu-priemka-sessions-{}.json",
                uuid::Uuid::new_v4()
            )))
        }
    }

    impl Drop for TestFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[tokio::test]
    async fn restores_session_with_credentials_from_debug_file() {
        let file = TestFile::new();
        let service =
            SessionService::with_persistence(Duration::from_secs(60), Some(file.0.clone()));
        let (id, _) = service
            .create(
                LdapIdentity {
                    username: "admin".to_owned(),
                },
                LdapCredentials::new("admin".to_owned(), "secret".to_owned()),
            )
            .await
            .expect("сессия должна создаться");
        drop(service);

        let restored =
            SessionService::with_persistence(Duration::from_secs(60), Some(file.0.clone()))
                .get(&id)
                .await
                .expect("сессия должна восстановиться");

        assert_eq!(restored.username, "admin");
        assert_eq!(restored.ldap_credentials.identifier(), "admin");
        assert_eq!(restored.ldap_credentials.password(), "secret");
    }

    #[tokio::test]
    async fn removed_session_is_removed_from_debug_file() {
        let file = TestFile::new();
        let service =
            SessionService::with_persistence(Duration::from_secs(60), Some(file.0.clone()));
        let (id, _) = service
            .create(
                LdapIdentity {
                    username: "admin".to_owned(),
                },
                LdapCredentials::new("admin".to_owned(), "secret".to_owned()),
            )
            .await
            .expect("сессия должна создаться");
        service.remove(&id).await;
        drop(service);

        let restored =
            SessionService::with_persistence(Duration::from_secs(60), Some(file.0.clone()));
        assert!(matches!(
            restored.get(&id).await,
            Err(AppError::Unauthorized)
        ));
    }
}
