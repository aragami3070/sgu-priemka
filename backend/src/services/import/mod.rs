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
        import::{ImportContext, PreparedStudent},
        job::{JobStage, JobStatus, LoginConflict, LoginResolution, ResultReference},
    },
    errors::{AppError, ImportError},
    services::{jobs::JobService, ldap::LdapService, results::ResultService},
};

use self::{
    credentials::{generate_password, normalize_conflict_login},
    parser::parse_csv,
    validation::{find_login_collisions, validate_students},
};

const LOGIN_RESOLUTION_TIMEOUT: Duration = Duration::from_secs(10 * 60);

enum LoginResolutionResult {
    Resolved(Vec<LoginResolution>),
    TimedOut(JobStatus),
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
        context: ImportContext,
        file_bytes: Vec<u8>,
    ) -> Result<JobStatus, AppError> {
        tracing::info!(
            job_id = %context.job_id,
            username = %context.username,
            filename = %context.original_filename,
            file_size = file_bytes.len(),
            "import pipeline started without LDAP stages"
        );

        self.publish_progress(&context.job_id, JobStage::Parsing, 0, 0)
            .await?;
        let parsing = tokio::task::spawn_blocking(move || parse_csv(&file_bytes)).await;
        let students = match parsing {
            Ok(Ok(students)) => students,
            Ok(Err(error)) => return self.finish_import_error(&context.job_id, error).await,
            Err(error) => {
                tracing::error!(job_id = %context.job_id, %error, "CSV parsing task failed");
                return self
                    .finish_internal_failure(
                        &context.job_id,
                        JobStage::Parsing,
                        "CSV parsing task failed",
                    )
                    .await;
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
            Ok(Err(error)) => return self.finish_import_error(&context.job_id, error).await,
            Err(error) => {
                tracing::error!(job_id = %context.job_id, %error, "CSV validation task failed");
                return self
                    .finish_internal_failure(
                        &context.job_id,
                        JobStage::Validating,
                        "CSV validation task failed",
                    )
                    .await;
            }
        };
        if let Some(status) = self
            .resolve_csv_login_conflicts(&context.job_id, &mut identities)
            .await?
        {
            return Ok(status);
        }
        self.publish_progress(&context.job_id, JobStage::Validating, total, total)
            .await?;

        self.publish_progress(&context.job_id, JobStage::GeneratingPasswords, 0, total)
            .await?;
        let salt = self.salt.salt.clone();
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
                tracing::error!(job_id = %context.job_id, %error, "password generation task failed");
                return self
                    .finish_internal_failure(
                        &context.job_id,
                        JobStage::GeneratingPasswords,
                        "password generation task failed",
                    )
                    .await;
            }
        };
        self.publish_progress(&context.job_id, JobStage::GeneratingPasswords, total, total)
            .await?;

        self.publish_progress(&context.job_id, JobStage::SavingResult, 0, 1)
            .await?;
        let stored = match self
            .results
            .create(&context.ldap_credentials, &students)
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
            "import pipeline completed without LDAP account creation"
        );
        Ok(status)
    }

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
}

#[cfg(test)]
mod tests {
    use std::{net::SocketAddr, path::PathBuf, time::Duration};

    use crate::{
        config::{LdapConfig, ResultConfig},
        entities::auth::LdapCredentials,
    };

    use super::*;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!(
                "sgu-priemka-import-service-{}",
                uuid::Uuid::new_v4()
            )))
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            if let Err(error) = std::fs::remove_dir_all(&self.0)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                panic!("не удалось удалить тестовый каталог: {error}");
            }
        }
    }

    fn config(directory: &TestDirectory) -> Arc<Config> {
        Arc::new(Config {
            listen_addr: SocketAddr::from(([127, 0, 0, 1], 8080)),
            cookie_secure: false,
            session_ttl: Duration::from_secs(60),
            ldap: LdapConfig {
                url: "ldap://ldap.test".to_owned(),
                user_bind_domain: "MAIN".to_owned(),
                auth_search_base_dn: "DC=main,DC=sgu,DC=ru".to_owned(),
                users_container_dn: "OU=Users,DC=main,DC=sgu,DC=ru".to_owned(),
                csit_admins_group_dn: "CN=Admins,DC=main,DC=sgu,DC=ru".to_owned(),
            },
            results: ResultConfig {
                output_dir: directory.0.clone(),
            },
            salt: "test-salt".to_owned(),
        })
    }

    fn service(directory: &TestDirectory) -> (ImportService, Arc<JobService>) {
        let config = config(directory);
        let jobs = Arc::new(JobService::new());
        let results = Arc::new(
            ResultService::new(config.clone()).expect("хранилище результатов должно создаться"),
        );
        let ldap = Arc::new(LdapService::new(config.clone()));
        (
            ImportService::new(ldap, jobs.clone(), results, config),
            jobs,
        )
    }

    async fn context(jobs: &JobService) -> ImportContext {
        let username = "admin".to_owned();
        let job_id = jobs
            .create(
                username.clone(),
                JobStatus::Progress {
                    stage: JobStage::Uploading,
                    current: 1,
                    total: 1,
                },
            )
            .await
            .expect("job должна создаться");
        ImportContext {
            job_id,
            username: username.clone(),
            ldap_credentials: Arc::new(LdapCredentials::new(username, "password".to_owned())),
            original_filename: "students.csv".to_owned(),
        }
    }

    #[tokio::test]
    async fn pipeline_without_ldap_creates_result_and_completes_job() {
        let directory = TestDirectory::new();
        let (service, jobs) = service(&directory);
        let context = context(&jobs).await;
        let mut events = jobs
            .subscribe(&context.job_id, &context.username)
            .await
            .expect("владелец должен подписаться");
        let csv = concat!(
            "First,Last,Patronymic,Email,Group\n",
            "Иван,Иванов,Иванович,ivan@example.com,111\n",
            "Пётр,Петров,Петрович,petr@example.com,121\n",
        );

        let status = service
            .run(context, csv.as_bytes().to_vec())
            .await
            .expect("pipeline должен завершиться");

        let JobStatus::Completed {
            created,
            total,
            result,
        } = status
        else {
            panic!("ожидался completed status")
        };
        assert_eq!(created, 0);
        assert_eq!(total, 2);
        assert!(
            directory
                .0
                .join(result.owner)
                .join(result.filename)
                .is_file()
        );
        events
            .changed()
            .await
            .expect("terminal status должен прийти");
        assert!(events.borrow_and_update().is_terminal());
    }

    #[tokio::test]
    async fn csv_login_collision_is_resolved_and_revalidated() {
        let directory = TestDirectory::new();
        let (service, jobs) = service(&directory);
        let context = context(&jobs).await;
        let job_id = context.job_id.clone();
        let username = context.username.clone();
        let mut events = jobs
            .subscribe(&job_id, &username)
            .await
            .expect("владелец должен подписаться");
        let csv = concat!(
            "First,Last,Patronymic,Email,Group\n",
            "Иван,Иванов,Иванович,ivan@example.com,111\n",
            "Игорь,Иванов,Ильич,igor@example.com,121\n",
            "Пётр,Петров,Петрович,petr@example.com,131\n",
            "Павел,Петров,Петрович,pavel@example.com,141\n",
        );

        let pipeline =
            tokio::spawn(async move { service.run(context, csv.as_bytes().to_vec()).await });

        loop {
            tokio::time::timeout(Duration::from_secs(1), events.changed())
                .await
                .expect("таймаут ожидания таблицы конфликтов")
                .expect("статус конфликта должен прийти");
            if let JobStatus::AwaitingLoginResolutions { conflicts } = &*events.borrow_and_update()
            {
                assert_eq!(
                    conflicts
                        .iter()
                        .map(|conflict| conflict.row)
                        .collect::<Vec<_>>(),
                    vec![2, 3, 4, 5]
                );
                assert_eq!(conflicts[0].full_name, "Иванов Иван Иванович");
                break;
            }
        }

        jobs.submit_login_resolutions(
            &job_id,
            &username,
            crate::entities::job::LoginResolutionBatch {
                resolutions: vec![
                    crate::entities::job::LoginResolution {
                        row: 2,
                        login: "ivanovii".to_owned(),
                    },
                    crate::entities::job::LoginResolution {
                        row: 3,
                        login: "ivanovii2".to_owned(),
                    },
                    crate::entities::job::LoginResolution {
                        row: 4,
                        login: "petrovpp".to_owned(),
                    },
                    crate::entities::job::LoginResolution {
                        row: 5,
                        login: "petrovpp".to_owned(),
                    },
                ],
            },
        )
        .await
        .expect("пакет замен должен быть принят на проверку");

        tokio::time::timeout(Duration::from_secs(1), events.changed())
            .await
            .expect("таймаут ожидания оставшихся конфликтов")
            .expect("оставшиеся конфликты должны прийти");
        {
            let status = events.borrow_and_update();
            let JobStatus::AwaitingLoginResolutions { conflicts } = &*status else {
                panic!("ожидалась обновлённая таблица конфликтов")
            };
            assert_eq!(
                conflicts
                    .iter()
                    .map(|conflict| conflict.row)
                    .collect::<Vec<_>>(),
                vec![4, 5]
            );
        }

        jobs.submit_login_resolutions(
            &job_id,
            &username,
            crate::entities::job::LoginResolutionBatch {
                resolutions: vec![
                    crate::entities::job::LoginResolution {
                        row: 4,
                        login: "petrovpp".to_owned(),
                    },
                    crate::entities::job::LoginResolution {
                        row: 5,
                        login: "petrovpp2".to_owned(),
                    },
                ],
            },
        )
        .await
        .expect("уникальная замена должна быть принята");

        let status = tokio::time::timeout(Duration::from_secs(1), pipeline)
            .await
            .expect("таймаут завершения pipeline")
            .expect("pipeline task не должна паниковать")
            .expect("pipeline должен завершиться");

        assert!(matches!(status, JobStatus::Completed { total: 4, .. }));
        let output = std::fs::read_to_string(
            std::fs::read_dir(directory.0.join("admin"))
                .expect("каталог результата должен читаться")
                .next()
                .expect("результат должен существовать")
                .expect("запись каталога должна читаться")
                .path(),
        )
        .expect("итоговый CSV должен читаться");
        assert!(output.contains("ivanovii2"));
        assert!(output.contains("petrovpp2"));
    }
}
