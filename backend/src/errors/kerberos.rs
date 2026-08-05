use std::{io, path::PathBuf};

use thiserror::Error;

/// Ошибки получения и использования персональных Kerberos credentials.
#[derive(Debug, Error)]
pub(crate) enum KerberosError {
    /// KDC отклонил principal или пароль пользователя.
    #[error("invalid Kerberos credentials")]
    InvalidCredentials,
    /// Локальный каталог или FILE ccache нельзя безопасно создать или удалить.
    #[error("Kerberos credential cache operation failed for `{path}`")]
    CacheIo {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// MIT libkrb5 завершил операцию ошибкой.
    #[error("Kerberos operation `{operation}` failed with code {code}: {message}")]
    Library {
        operation: &'static str,
        code: i32,
        message: String,
    },
    /// GSSAPI не смог импортировать credential из указанного ccache.
    #[error("GSSAPI credential import failed: {message}")]
    Gssapi { message: String },
    /// Blocking Kerberos-вызов не завершился штатно.
    #[error("Kerberos blocking task failed")]
    BlockingTask {
        #[source]
        source: tokio::task::JoinError,
    },
    /// Полученный TGT уже истёк или имеет некорректный срок.
    #[error("Kerberos returned an invalid TGT expiration time")]
    InvalidExpiration,
    /// Сформированный principal или cache name содержит NUL.
    #[error("Kerberos input `{field}` contains a NUL byte")]
    InteriorNul { field: &'static str },
    /// Случайный session ID столкнулся с уже существующим ccache.
    #[error("Kerberos credential cache already exists: `{0}`")]
    CacheAlreadyExists(PathBuf),
}

impl KerberosError {
    /// Создаёт ошибку файловой операции credential cache.
    pub(crate) fn cache_io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::CacheIo {
            path: path.into(),
            source,
        }
    }
}
