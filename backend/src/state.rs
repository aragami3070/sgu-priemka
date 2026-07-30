use std::sync::Arc;

use crate::{
    config::Config,
    errors::AppError,
    services::{
        import::ImportService, jobs::JobService, ldap::LdapService, results::ResultService,
        sessions::SessionService,
    },
};

/// Разделяемые зависимости HTTP-обработчиков и фоновых задач.
#[derive(Clone)]
pub(crate) struct AppState {
    /// Проверенная конфигурация процесса.
    pub(crate) config: Arc<Config>,
    /// Общий LDAP-сервис для входа операторов и служебных операций со студентами.
    pub(crate) ldap: Arc<LdapService>,
    /// Сервис локальных непрозрачных сессий.
    pub(crate) sessions: Arc<SessionService>,
    /// Сервис полного pipeline импорта.
    pub(crate) imports: Arc<ImportService>,
    /// Сервис задач импорта и их прогресса.
    pub(crate) jobs: Arc<JobService>,
    /// Сервис сформированных итоговых CSV.
    pub(crate) results: Arc<ResultService>,
}

impl AppState {
    /// Создаёт разделяемые сервисы из проверенной конфигурации.
    pub(crate) fn new(_config: Config) -> Result<Self, AppError> {
        todo!("construct shared application services")
    }
}
