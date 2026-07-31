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
    /// Общий LDAP-сервис для входа операторов и операций от имени текущей сессии.
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
    pub(crate) fn new(config: Config) -> Result<Self, AppError> {
        let config = Arc::new(config);

        // NOTE: Инициализация сервисов
        let ldap = Arc::new(LdapService::new(config.clone()));
        let sessions = Arc::new(SessionService::new(config.session_ttl));
        let jobs = Arc::new(JobService::new());
        let results = Arc::new(ResultService::new(config.clone())?);
        let imports = Arc::new(ImportService::new(
            ldap.clone(),
            jobs.clone(),
            results.clone(),
            config.clone(),
        ));

        let state = Self {
            config,
            ldap,
            sessions,
            jobs,
            imports,
            results,
        };
        tracing::info!("application state initialization completed");
        Ok(state)
    }
}
