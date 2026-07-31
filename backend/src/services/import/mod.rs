//! Сервис импорта и его последовательные этапы обработки.

mod credentials;
mod parser;
mod validation;

use std::sync::Arc;

use tokio::sync::Semaphore;

use crate::{
    config::Config,
    entities::{import::ImportContext, job::JobStatus},
    errors::AppError,
    services::{jobs::JobService, ldap::LdapService, results::ResultService},
};

/// Управляет полным pipeline импорта студентов.
pub(crate) struct ImportService {
    /// Сервис поиска конфликтов и создания учётных записей через LDAP.
    ldap: Arc<LdapService>,
    /// Сервис публикации прогресса задачи.
    jobs: Arc<JobService>,
    /// Сервис записи итоговых CSV.
    results: Arc<ResultService>,
    /// Для доступа к соли для вычисления временных паролей.
    salt: Arc<Config>,
    /// Блокировка, исключающая параллельную запись нескольких импортов в LDAP.
    lock: Arc<Semaphore>,
}

impl ImportService {
    /// Собирает pipeline из разделяемых прикладных сервисов.
    pub(crate) fn new(
        ldap: Arc<LdapService>,
        jobs: Arc<JobService>,
        results: Arc<ResultService>,
        salt: Arc<Config>,
    ) -> Self {
        Self {
            ldap,
            jobs,
            results,
            salt,
            lock: Arc::new(Semaphore::new(1)),
        }
    }

    /// Выполняет разбор, валидацию, поиск конфликтов, генерацию паролей, LDAP и вывод CSV.
    pub(crate) async fn run(
        &self,
        _context: ImportContext,
        _file_bytes: Vec<u8>,
    ) -> Result<JobStatus, AppError> {
        todo!("execute the validated import pipeline and publish progress")
    }
}
