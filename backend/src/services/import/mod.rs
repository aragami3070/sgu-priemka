//! Сервис импорта и его последовательные этапы обработки.

mod credentials;
mod parser;
mod validation;

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use tokio::sync::Semaphore;

use crate::{
    config::Config,
    entities::{
        import::{ImportContext, PreparedIdentity, PreparedStudent},
        job::{JobStage, JobStatus, LoginConflict, LoginResolution, ResultReference},
    },
    errors::{AppError, ImportError, LdapError},
    services::{jobs::JobService, ldap::LdapService, results::ResultService},
};

use self::{
    credentials::{generate_password, normalize_conflict_login},
    parser::{parse_csv, parse_result_csv},
    validation::{find_login_collisions, validate_students},
};

const LOGIN_RESOLUTION_TIMEOUT: Duration = Duration::from_secs(10 * 60);

enum LoginResolutionResult {
    Resolved(Vec<LoginResolution>),
    TimedOut(JobStatus),
}

/// Формирует ФИО студента в порядке, используемом LDAP DN и статусом job.
fn student_full_name(student: &PreparedStudent) -> String {
    let source = &student.identity.source;
    format!(
        "{} {} {}",
        source.last_name.trim(),
        source.first_name.trim(),
        source.patronymic.trim()
    )
}

/// Возвращает безопасные сведения о частично выполненной LDAP-операции.
fn ldap_failure_details(error: &LdapError) -> (String, bool) {
    match error {
        LdapError::Operation {
            phase,
            possibly_created,
            ..
        } => (format!("{phase:?}"), *possibly_created),
        _ => ("Unknown".to_owned(), false),
    }
}

enum PipelineStage<T> {
    Continue(T),
    Finished(JobStatus),
}

/// Управляет полным pipeline импорта студентов.
pub(crate) struct ImportService {
    /// Сервис поиска конфликтов и создания учётных записей через LDAP.
    ldap: Arc<LdapService>,
    /// Сервис публикации прогресса задачи.
    jobs: Arc<JobService>,
    /// Сервис записи итоговых CSV.
    results: Arc<ResultService>,
    /// Для доступа к соли для вычисления временных паролей.
    config: Arc<Config>,
    /// Блокировка, исключающая параллельную запись нескольких импортов в LDAP.
    lock: Arc<Semaphore>,
}

impl ImportService {
    /// Собирает pipeline из разделяемых прикладных сервисов.
    pub(crate) fn new(
        ldap: Arc<LdapService>,
        jobs: Arc<JobService>,
        results: Arc<ResultService>,
        config: Arc<Config>,
    ) -> Self {
        Self {
            ldap,
            jobs,
            results,
            config,
            lock: Arc::new(Semaphore::new(1)),
        }
    }

    /// Выполняет разбор, валидацию, поиск конфликтов, генерацию паролей, LDAP и вывод CSV.
    pub(crate) async fn run(
        &self,
        context: ImportContext,
        file_bytes: Vec<u8>,
    ) -> Result<JobStatus, AppError> {
        tracing::info!(
            job_id = %context.job_id,
            username = %context.username,
            filename = %context.original_filename,
            file_size = file_bytes.len(),
            "import pipeline started"
        );

        let (mut identities, total) = match self.parse_and_validate(&context, file_bytes).await? {
            PipelineStage::Continue(value) => value,
            PipelineStage::Finished(status) => return Ok(status),
        };

        identities = match self.check_ldap(&context, &mut identities, total).await? {
            PipelineStage::Continue(value) => value,
            PipelineStage::Finished(status) => return Ok(status),
        };

        let students = match self
            .generate_passwords(&context.job_id, identities, total)
            .await?
        {
            PipelineStage::Continue(value) => value,
            PipelineStage::Finished(status) => return Ok(status),
        };

        self.publish_progress(&context.job_id, JobStage::SavingResult, 0, 1)
            .await?;
        let stored = match self
            .results
            .create(&context.kerberos_credentials, &students)
            .await
        {
            Ok(stored) => stored,
            Err(error) => {
                tracing::error!(job_id = %context.job_id, %error, "result CSV creation failed");
                return self
                    .finish_internal_failure(
                        &context.job_id,
                        JobStage::SavingResult,
                        "result CSV creation failed",
                    )
                    .await;
            }
        };

        let status = JobStatus::Completed {
            created: 0,
            total,
            result: ResultReference {
                owner: stored.owner,
                filename: stored.filename,
            },
        };
        self.jobs.publish(&context.job_id, status.clone()).await?;
        tracing::info!(
            job_id = %context.job_id,
            username = %context.username,
            prepared_students = total,
            "import pipeline completed with validated account preparation; LDAP account creation is pending"
        );
        Ok(status)
    }

    /// Запускает LDAP-создание по уже сохранённому итоговому CSV.
    pub(crate) async fn run_result(
        &self,
        context: ImportContext,
        result_owner: String,
        result_filename: String,
    ) -> Result<JobStatus, AppError> {
        tracing::info!(
            job_id = %context.job_id,
            result_owner = %result_owner,
            result_filename = %result_filename,
            "LDAP creation from stored result started"
        );
        let mut students = match self
            .load_result_students(&context, &result_owner, &result_filename)
            .await?
        {
            PipelineStage::Continue(students) => students,
            PipelineStage::Finished(status) => return Ok(status),
        };
        let total = students.len();
        let _creation_lock = self.lock.acquire().await.map_err(|_| AppError::Internal)?;

        let mut identities = students
            .iter()
            .map(|student| student.identity.clone())
            .collect::<Vec<_>>();
        if let Some(status) = self
            .resolve_csv_login_conflicts(&context.job_id, &mut identities)
            .await?
        {
            return Ok(status);
        }
        match self.check_ldap(&context, &mut identities, total).await? {
            PipelineStage::Continue(updated) => {
                for (student, identity) in students.iter_mut().zip(updated) {
                    student.identity = identity;
                }
            }
            PipelineStage::Finished(status) => return Ok(status),
        }

        self.create_accounts(
            &context,
            &students,
            total,
            ResultReference {
                owner: result_owner,
                filename: result_filename,
            },
        )
        .await
    }

    /// Загружает и разбирает сохранённый CSV перед LDAP-созданием.
    async fn load_result_students(
        &self,
        context: &ImportContext,
        owner: &str,
        filename: &str,
    ) -> Result<PipelineStage<Vec<PreparedStudent>>, AppError> {
        self.publish_progress(&context.job_id, JobStage::Parsing, 0, 0)
            .await?;
        let bytes = self.results.read(owner, filename).await?;
        let parsing = tokio::task::spawn_blocking(move || parse_result_csv(&bytes)).await;
        let students = match parsing {
            Ok(Ok(students)) => students,
            Ok(Err(error)) => {
                return Ok(PipelineStage::Finished(
                    self.finish_import_error(&context.job_id, error).await?,
                ));
            }
            Err(error) => {
                tracing::error!(job_id = %context.job_id, %error, "stored result parsing task failed");
                return Ok(PipelineStage::Finished(
                    self.finish_internal_failure(
                        &context.job_id,
                        JobStage::Parsing,
                        "stored result parsing task failed",
                    )
                    .await?,
                ));
            }
        };
        self.publish_progress(
            &context.job_id,
            JobStage::Parsing,
            students.len(),
            students.len(),
        )
        .await?;
        Ok(PipelineStage::Continue(students))
    }

    /// Последовательно создаёт студентов из результата и публикует частичный статус при сбое.
    async fn create_accounts(
        &self,
        context: &ImportContext,
        students: &[PreparedStudent],
        total: usize,
        result: ResultReference,
    ) -> Result<JobStatus, AppError> {
        self.publish_progress(&context.job_id, JobStage::CreatingAccounts, 0, total)
            .await?;
        for (index, student) in students.iter().enumerate() {
            if let Err(error) = self
                .ldap
                .create_user(&context.kerberos_credentials, student)
                .await
            {
                let (ldap_phase, possibly_created) = ldap_failure_details(&error);
                let status = JobStatus::PartialFailure {
                    created: index,
                    total,
                    failed_row: student.identity.source.source_row,
                    failed_fio: student_full_name(student),
                    ldap_phase,
                    possibly_created,
                    result,
                };
                tracing::warn!(
                    job_id = %context.job_id,
                    row = student.identity.source.source_row,
                    login = %student.identity.login,
                    error = ?error,
                    "LDAP account creation stopped after partial progress"
                );
                self.jobs.publish(&context.job_id, status.clone()).await?;
                return Ok(status);
            }
            self.publish_progress(
                &context.job_id,
                JobStage::CreatingAccounts,
                index + 1,
                total,
            )
            .await?;
        }

        let status = JobStatus::Completed {
            created: students.len(),
            total,
            result,
        };
        self.jobs.publish(&context.job_id, status.clone()).await?;
        tracing::info!(
            job_id = %context.job_id,
            username = %context.username,
            created_students = students.len(),
            "LDAP creation from stored result completed"
        );
        Ok(status)
    }

    /// Разбирает CSV, валидирует строки и разрешает конфликты логинов внутри файла.
    async fn parse_and_validate(
        &self,
        context: &ImportContext,
        file_bytes: Vec<u8>,
    ) -> Result<PipelineStage<(Vec<PreparedIdentity>, usize)>, AppError> {
        self.publish_progress(&context.job_id, JobStage::Parsing, 0, 0)
            .await?;
        let parsing = tokio::task::spawn_blocking(move || parse_csv(&file_bytes)).await;
        let students = match parsing {
            Ok(Ok(students)) => students,
            Ok(Err(error)) => {
                return Ok(PipelineStage::Finished(
                    self.finish_import_error(&context.job_id, error).await?,
                ));
            }
            Err(error) => {
                tracing::error!(job_id = %context.job_id, %error, "CSV parsing task failed");
                return Ok(PipelineStage::Finished(
                    self.finish_internal_failure(
                        &context.job_id,
                        JobStage::Parsing,
                        "CSV parsing task failed",
                    )
                    .await?,
                ));
            }
        };
        let total = students.len();
        self.publish_progress(&context.job_id, JobStage::Parsing, total, total)
            .await?;
        self.publish_progress(&context.job_id, JobStage::Validating, 0, total)
            .await?;

        let validation = tokio::task::spawn_blocking(move || validate_students(students)).await;
        let mut identities = match validation {
            Ok(Ok(identities)) => identities,
            Ok(Err(error)) => {
                return Ok(PipelineStage::Finished(
                    self.finish_import_error(&context.job_id, error).await?,
                ));
            }
            Err(error) => {
                tracing::error!(job_id = %context.job_id, %error, "CSV validation task failed");
                return Ok(PipelineStage::Finished(
                    self.finish_internal_failure(
                        &context.job_id,
                        JobStage::Validating,
                        "CSV validation task failed",
                    )
                    .await?,
                ));
            }
        };
        if let Some(status) = self
            .resolve_csv_login_conflicts(&context.job_id, &mut identities)
            .await?
        {
            return Ok(PipelineStage::Finished(status));
        }
        self.publish_progress(&context.job_id, JobStage::Validating, total, total)
            .await?;
        Ok(PipelineStage::Continue((identities, total)))
    }

    /// Проверяет сгенерированные логины в LDAP и разрешает найденные конфликты.
    async fn check_ldap(
        &self,
        context: &ImportContext,
        identities: &mut [PreparedIdentity],
        total: usize,
    ) -> Result<PipelineStage<Vec<PreparedIdentity>>, AppError> {
        self.publish_progress(&context.job_id, JobStage::CheckingLdap, 0, total)
            .await?;
        if let Some(status) = self
            .resolve_ldap_login_conflicts(context, identities)
            .await?
        {
            return Ok(PipelineStage::Finished(status));
        }
        self.publish_progress(&context.job_id, JobStage::CheckingLdap, total, total)
            .await?;
        Ok(PipelineStage::Continue(identities.to_vec()))
    }

    /// Генерирует временные пароли для проверенных identity в blocking-задаче.
    async fn generate_passwords(
        &self,
        job_id: &str,
        identities: Vec<PreparedIdentity>,
        total: usize,
    ) -> Result<PipelineStage<Vec<PreparedStudent>>, AppError> {
        self.publish_progress(job_id, JobStage::GeneratingPasswords, 0, total)
            .await?;
        let salt = self.config.salt.clone();
        let students = tokio::task::spawn_blocking(move || {
            identities
                .into_iter()
                .map(|identity| {
                    let uuid = uuid::Uuid::new_v4().to_string();
                    let password = generate_password(&identity.login, &uuid, &salt);
                    PreparedStudent { identity, password }
                })
                .collect::<Vec<_>>()
        })
        .await;
        let students = match students {
            Ok(students) => students,
            Err(error) => {
                tracing::error!(job_id = %job_id, %error, "password generation task failed");
                return Ok(PipelineStage::Finished(
                    self.finish_internal_failure(
                        job_id,
                        JobStage::GeneratingPasswords,
                        "password generation task failed",
                    )
                    .await?,
                ));
            }
        };
        self.publish_progress(job_id, JobStage::GeneratingPasswords, total, total)
            .await?;
        Ok(PipelineStage::Continue(students))
    }

    /// Публикует промежуточный статус pipeline для подписчиков задачи.
    async fn publish_progress(
        &self,
        job_id: &str,
        stage: JobStage,
        current: usize,
        total: usize,
    ) -> Result<(), AppError> {
        self.jobs
            .publish(
                job_id,
                JobStatus::Progress {
                    stage,
                    current,
                    total,
                },
            )
            .await
    }

    /// Запрашивает у frontend пакет замен всех конфликтующих внутри CSV логинов.
    ///
    /// После каждого ответа весь набор проверяется заново, поэтому старое или уже
    /// занятое другой строкой значение снова приводит к тому же запросу.
    async fn resolve_csv_login_conflicts(
        &self,
        job_id: &str,
        identities: &mut [crate::entities::import::PreparedIdentity],
    ) -> Result<Option<JobStatus>, AppError> {
        loop {
            let indices = find_login_collisions(identities);
            if indices.is_empty() {
                return Ok(None);
            }
            let conflicts = indices
                .iter()
                .map(|&index| {
                    let identity = &identities[index];
                    LoginConflict {
                        row: identity.source.source_row,
                        full_name: format!(
                            "{} {} {}",
                            identity.source.last_name.trim(),
                            identity.source.first_name.trim(),
                            identity.source.patronymic.trim()
                        ),
                        login: identity.login.clone(),
                        message: format!(
                            "Логин `{}` используется несколькими строками этого CSV",
                            identity.login
                        ),
                    }
                })
                .collect();

            match self
                .request_login_resolution(job_id, JobStage::Validating, conflicts)
                .await?
            {
                LoginResolutionResult::Resolved(resolutions) => {
                    let resolutions = resolutions
                        .into_iter()
                        .map(|resolution| (resolution.row, resolution.login))
                        .collect::<HashMap<_, _>>();
                    for &index in &indices {
                        let identity = &mut identities[index];
                        if let Some(login) = resolutions.get(&identity.source.source_row) {
                            identity.login.clone_from(login);
                        }
                    }
                    tracing::info!(
                        %job_id,
                        submitted_logins = resolutions.len(),
                        "replacement login batch accepted for repeated validation"
                    );
                }
                LoginResolutionResult::TimedOut(status) => return Ok(Some(status)),
            }
        }
    }

    /// Ищет уже занятые в LDAP логины и повторно запрашивает их исправление.
    async fn resolve_ldap_login_conflicts(
        &self,
        context: &ImportContext,
        identities: &mut [PreparedIdentity],
    ) -> Result<Option<JobStatus>, AppError> {
        loop {
            let collisions = match self
                .ldap
                .find_collisions(&context.kerberos_credentials, identities)
                .await
            {
                Ok(collisions) => collisions,
                Err(error) => {
                    tracing::warn!(
                        job_id = %context.job_id,
                        error = ?error,
                        "LDAP login collision search failed"
                    );
                    return self
                        .finish_ldap_failure(&context.job_id, error)
                        .await
                        .map(Some);
                }
            };
            if collisions.is_empty() {
                return Ok(None);
            }

            let conflicts = collisions
                .iter()
                .filter_map(|collision| {
                    identities
                        .iter()
                        .find(|identity| identity.source.source_row == collision.source_row)
                        .map(|identity| LoginConflict {
                            row: collision.source_row,
                            full_name: format!(
                                "{} {} {}",
                                identity.source.last_name.trim(),
                                identity.source.first_name.trim(),
                                identity.source.patronymic.trim()
                            ),
                            login: collision.value.clone(),
                            message: format!("Логин `{}` уже существует в LDAP", collision.value),
                        })
                })
                .collect();

            match self
                .request_login_resolution(&context.job_id, JobStage::CheckingLdap, conflicts)
                .await?
            {
                LoginResolutionResult::Resolved(resolutions) => {
                    for resolution in resolutions {
                        if let Some(identity) = identities
                            .iter_mut()
                            .find(|identity| identity.source.source_row == resolution.row)
                        {
                            identity.login = resolution.login;
                        }
                    }
                    // Замена, предложенная для LDAP-конфликта, может создать
                    // дубликат внутри самого CSV — проверяем его тем же диалогом.
                    if let Some(status) = self
                        .resolve_csv_login_conflicts(&context.job_id, identities)
                        .await?
                    {
                        return Ok(Some(status));
                    }
                }
                LoginResolutionResult::TimedOut(status) => return Ok(Some(status)),
            }
        }
    }

    /// Единый WebSocket-диалог для конфликтов внутри CSV и будущих LDAP-конфликтов.
    async fn request_login_resolution(
        &self,
        job_id: &str,
        stage: JobStage,
        mut conflicts: Vec<LoginConflict>,
    ) -> Result<LoginResolutionResult, AppError> {
        loop {
            self.jobs
                .publish(
                    job_id,
                    JobStatus::AwaitingLoginResolutions {
                        conflicts: conflicts.clone(),
                    },
                )
                .await?;
            tracing::info!(
                %job_id,
                conflicts = conflicts.len(),
                "import pipeline is waiting for login conflict resolution batch"
            );

            let batch = match tokio::time::timeout(
                LOGIN_RESOLUTION_TIMEOUT,
                self.jobs.wait_for_login_resolutions(job_id),
            )
            .await
            {
                Ok(resolution) => resolution?,
                Err(_) => {
                    let status = JobStatus::Failed {
                        stage,
                        code: "login_resolution_timeout".to_owned(),
                        message: "Время ожидания исправлений логинов истекло".to_owned(),
                        row: None,
                    };
                    self.jobs.publish(job_id, status.clone()).await?;
                    tracing::warn!(%job_id, "login conflict resolution batch timed out");
                    return Ok(LoginResolutionResult::TimedOut(status));
                }
            };

            let expected_rows = conflicts
                .iter()
                .map(|conflict| conflict.row)
                .collect::<HashSet<_>>();
            let mut submitted = HashMap::with_capacity(batch.resolutions.len());
            let mut duplicated_rows = HashSet::new();
            for resolution in batch.resolutions {
                if expected_rows.contains(&resolution.row)
                    && submitted.insert(resolution.row, resolution.login).is_some()
                {
                    duplicated_rows.insert(resolution.row);
                }
            }

            let mut normalized = Vec::with_capacity(conflicts.len());
            let mut has_errors = false;
            for conflict in &mut conflicts {
                if duplicated_rows.contains(&conflict.row) {
                    conflict.message = "Для строки передано несколько логинов".to_owned();
                    has_errors = true;
                    continue;
                }
                let Some(login) = submitted.remove(&conflict.row) else {
                    conflict.message = "Введите логин для этой строки".to_owned();
                    has_errors = true;
                    continue;
                };
                conflict.login = login.trim().to_owned();
                match normalize_conflict_login(conflict.row, &login) {
                    Ok(login) => normalized.push(LoginResolution {
                        row: conflict.row,
                        login,
                    }),
                    Err(ImportError::Validation {
                        message: validation_message,
                        ..
                    }) => {
                        conflict.message = validation_message;
                        has_errors = true;
                    }
                    Err(error) => {
                        conflict.message = error.to_string();
                        has_errors = true;
                    }
                }
            }

            if !has_errors {
                return Ok(LoginResolutionResult::Resolved(normalized));
            }
        }
    }

    /// Преобразует ошибку разбора/валидации в финальный статус задачи.
    async fn finish_import_error(
        &self,
        job_id: &str,
        error: ImportError,
    ) -> Result<JobStatus, AppError> {
        let (stage, code, row) = match &error {
            ImportError::Decode => (JobStage::Parsing, "csv_decode", None),
            ImportError::Parse { row, .. } => (JobStage::Parsing, "csv_parse", Some(*row)),
            ImportError::Validation { row, .. } => {
                (JobStage::Validating, "csv_validation", Some(*row))
            }
            ImportError::UnsupportedGroup { row, .. } => {
                (JobStage::Validating, "unsupported_group", Some(*row))
            }
            ImportError::Collision { row, .. } => {
                (JobStage::Validating, "csv_collision", Some(*row))
            }
            ImportError::ResultStorage => (JobStage::SavingResult, "result_storage", None),
        };
        let status = JobStatus::Failed {
            stage,
            code: code.to_owned(),
            message: error.to_string(),
            row,
        };
        self.jobs.publish(job_id, status.clone()).await?;
        tracing::warn!(%job_id, %error, "import pipeline failed before LDAP stages");
        Ok(status)
    }

    /// Публикует финальную ошибку внутреннего этапа pipeline.
    async fn finish_internal_failure(
        &self,
        job_id: &str,
        stage: JobStage,
        message: &str,
    ) -> Result<JobStatus, AppError> {
        let status = JobStatus::Failed {
            stage,
            code: "internal_error".to_owned(),
            message: message.to_owned(),
            row: None,
        };
        self.jobs.publish(job_id, status.clone()).await?;
        Ok(status)
    }

    /// Завершает задачу стабильным статусом при недоступности LDAP.
    async fn finish_ldap_failure(
        &self,
        job_id: &str,
        error: crate::errors::LdapError,
    ) -> Result<JobStatus, AppError> {
        let status = JobStatus::Failed {
            stage: JobStage::CheckingLdap,
            code: "ldap_unavailable".to_owned(),
            message: "LDAP is unavailable".to_owned(),
            row: None,
        };
        tracing::warn!(%job_id, error = ?error, "import pipeline stopped during LDAP check");
        self.jobs.publish(job_id, status.clone()).await?;
        Ok(status)
    }
}
