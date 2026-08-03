//! Сервис импорта и его последовательные этапы обработки.

mod credentials;
mod parser;
mod validation;

use std::sync::Arc;

use tokio::sync::Semaphore;

use crate::{
    config::Config,
    entities::{
        import::{ImportContext, PreparedStudent},
        job::{JobStage, JobStatus, ResultReference},
    },
    errors::{AppError, ImportError},
    services::{jobs::JobService, ldap::LdapService, results::ResultService},
};

use self::{credentials::generate_password, parser::parse_csv, validation::validate_students};

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
        let identities = match validation {
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
                ttl: Duration::from_secs(60),
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
            "Иван,Иванов,Иванович,ivan@example.com,001\n",
            "Пётр,Петров,Петрович,petr@example.com,002\n",
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
    async fn csv_collision_fails_job_without_creating_result() {
        let directory = TestDirectory::new();
        let (service, jobs) = service(&directory);
        let context = context(&jobs).await;
        let csv = concat!(
            "First,Last,Patronymic,Email,Group\n",
            "Иван,Иванов,Иванович,ivan@example.com,001\n",
            "Игорь,Иванов,Ильич,igor@example.com,002\n",
        );

        let status = service
            .run(context, csv.as_bytes().to_vec())
            .await
            .expect("validation failure должна быть terminal status");

        assert!(matches!(
            status,
            JobStatus::Failed {
                stage: JobStage::Validating,
                code,
                row: Some(3),
                ..
            } if code == "csv_collision"
        ));
        assert!(
            std::fs::read_dir(&directory.0)
                .expect("корневой каталог должен читаться")
                .next()
                .is_none()
        );
    }
}
