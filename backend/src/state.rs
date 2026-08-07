use std::sync::Arc;

use crate::{
    config::Config,
    errors::AppError,
    services::{
        import::ImportService, jobs::JobService, kerberos::KerberosService, ldap::LdapService,
        mail::MailService, results::ResultService, sessions::SessionService,
    },
};

/// Разделяемые зависимости HTTP-обработчиков и фоновых задач.
#[derive(Clone)]
pub(crate) struct AppState {
    /// Проверенная конфигурация процесса.
    pub(crate) config: Arc<Config>,
    /// Общий LDAP-сервис для входа операторов и операций от имени текущей сессии.
    pub(crate) ldap: Arc<LdapService>,
    /// Получение TGT, explicit GSSAPI credentials и очистка session ccache.
    pub(crate) kerberos: Arc<KerberosService>,
    /// Сервис локальных непрозрачных сессий.
    pub(crate) sessions: Arc<SessionService>,
    /// Сервис полного pipeline импорта.
    pub(crate) imports: Arc<ImportService>,
    /// Сервис задач импорта и их прогресса.
    pub(crate) jobs: Arc<JobService>,
    /// Сервис сформированных итоговых CSV.
    pub(crate) results: Arc<ResultService>,
    /// Общий SMTP-сервис для рассылки credentials.
    pub(crate) mail: Arc<MailService>,
}

impl AppState {
    /// Создаёт разделяемые сервисы из проверенной конфигурации.
    pub(crate) fn new(config: Config) -> Result<Self, AppError> {
        let config = Arc::new(config);

        // NOTE: Инициализация сервисов
        let kerberos = Arc::new(KerberosService::new(config.clone())?);
        let ldap = Arc::new(LdapService::new(config.clone(), kerberos.clone()));
        let sessions = Arc::new(SessionService::new(config.session_ttl, kerberos.clone()));
        let jobs = Arc::new(JobService::new());
        let results = Arc::new(ResultService::new(config.clone())?);
        let mail = Arc::new(MailService::new(&config.mail).map_err(|error| {
            tracing::error!(%error, "не удалось инициализировать почтовый сервис");
            AppError::Internal
        })?);
        let imports = Arc::new(ImportService::new(
            ldap.clone(),
            jobs.clone(),
            results.clone(),
            config.clone(),
        ));

        let state = Self {
            config,
            ldap,
            kerberos,
            sessions,
            jobs,
            imports,
            results,
            mail,
        };
        tracing::info!("инициализация состояния приложения завершена");
        Ok(state)
    }
}
