use std::{
    cmp::Reverse,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

use csv::{Terminator, WriterBuilder};
use time::OffsetDateTime;
use tokio::{fs, io::AsyncWriteExt};
use uuid::Uuid;

use crate::{
    config::Config,
    entities::{auth::LdapCredentials, import::PreparedStudent, result::StoredResult},
    errors::{AppError, ResultError},
};

/// Файловый сервис для сформированных CSV с учётными данными.
pub(crate) struct ResultService {
    config: Arc<Config>,
}

struct ResultEntry {
    path: PathBuf,
    filename: String,
    metadata: std::fs::Metadata,
}

impl ResultService {
    /// Создаёт при необходимости выходной каталог и подготавливает хранилище.
    pub(crate) fn new(config: Arc<Config>) -> Result<Self, AppError> {
        std::fs::create_dir_all(&config.results.output_dir).map_err(|error| {
            storage_error(
                "initialize result storage",
                &config.results.output_dir,
                error,
            )
        })?;

        tracing::info!(
            output_dir = %config.results.output_dir.display(),
            "result storage initialized"
        );
        Ok(Self { config })
    }

    /// Атомарно записывает CSV в каталог LDAP-пользователя, запустившего импорт.
    pub(crate) async fn create(
        &self,
        credentials: &LdapCredentials,
        students: &[PreparedStudent],
    ) -> Result<StoredResult, AppError> {
        let owner = credentials.identifier();
        validate_path_segment(owner, "result owner")?;

        let created_at = OffsetDateTime::now_utc();
        let filename = result_filename(created_at);
        let owner_dir = self.config.results.output_dir.join(owner);
        let path = owner_dir.join(&filename);
        let temporary_path = owner_dir.join(format!(".{filename}.{}.tmp", Uuid::new_v4()));
        let bytes = serialize_students(students)?;

        fs::create_dir_all(&owner_dir)
            .await
            .map_err(|error| storage_error("create result owner directory", &owner_dir, error))?;

        let write_result = async {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary_path)
                .await?;
            file.write_all(&bytes).await?;
            file.sync_all().await?;
            drop(file);
            fs::rename(&temporary_path, &path).await
        }
        .await;

        if let Err(error) = write_result {
            if let Err(cleanup_error) = fs::remove_file(&temporary_path).await
                && cleanup_error.kind() != ErrorKind::NotFound
            {
                tracing::warn!(
                    path = %temporary_path.display(),
                    %cleanup_error,
                    "failed to remove temporary result file"
                );
            }
            return Err(storage_error("atomically write result", &path, error).into());
        }

        tracing::info!(
            owner,
            %filename,
            student_count = students.len(),
            size = bytes.len(),
            "result CSV created"
        );

        Ok(StoredResult {
            owner: owner.to_owned(),
            filename,
            path,
            created_at,
            size: bytes.len() as u64,
        })
    }

    /// Возвращает результаты всех администраторов, которые ещё не истекли.
    pub(crate) async fn list(&self) -> Result<Vec<StoredResult>, AppError> {
        let mut results = Vec::new();
        let mut owners = fs::read_dir(&self.config.results.output_dir)
            .await
            .map_err(|error| {
                storage_error(
                    "scan result storage",
                    &self.config.results.output_dir,
                    error,
                )
            })?;

        while let Some(owner_entry) = owners.next_entry().await.map_err(|error| {
            storage_error(
                "read result owner entry",
                &self.config.results.output_dir,
                error,
            )
        })? {
            if !owner_entry
                .file_type()
                .await
                .map_err(|error| storage_error("inspect result owner", &owner_entry.path(), error))?
                .is_dir()
            {
                continue;
            }

            let Some(owner) = owner_entry.file_name().to_str().map(str::to_owned) else {
                tracing::warn!(path = %owner_entry.path().display(), "skipping non-UTF-8 result owner directory");
                continue;
            };
            if validate_path_segment(&owner, "result owner").is_err() {
                tracing::warn!(%owner, "skipping invalid result owner directory");
                continue;
            }

            self.collect_owner_results(&owner, &owner_entry.path(), &mut results)
                .await?;
        }

        results.sort_unstable_by_key(|result| Reverse(result.created_at));
        tracing::info!(result_count = results.len(), "result list collected");
        Ok(results)
    }

    /// Читает результат после проверки имени каталога владельца и имени файла.
    pub(crate) async fn read(&self, owner: &str, filename: &str) -> Result<Vec<u8>, AppError> {
        validate_path_segment(owner, "result owner")?;
        validate_result_filename(filename)?;

        let path = self.config.results.output_dir.join(owner).join(filename);
        let metadata = match fs::symlink_metadata(&path).await {
            Ok(metadata) if metadata.file_type().is_file() => metadata,
            Ok(_) => return Err(AppError::NotFound),
            Err(error) if error.kind() == ErrorKind::NotFound => return Err(AppError::NotFound),
            Err(error) => return Err(storage_error("inspect result", &path, error).into()),
        };

        if is_expired(&metadata, self.config.results.ttl)? {
            tracing::info!(%owner, %filename, "expired result read rejected");
            return Err(AppError::NotFound);
        }

        let bytes = fs::read(&path)
            .await
            .map_err(|error| storage_error("read result", &path, error))?;
        tracing::info!(%owner, %filename, size = bytes.len(), "result CSV read");
        Ok(bytes)
    }

    /// Удаляет результаты старше настроенного срока хранения и пустые каталоги владельцев.
    pub(crate) async fn cleanup_expired(&self) -> Result<(), AppError> {
        let mut removed_files = 0usize;
        let mut removed_directories = 0usize;
        let mut owners = fs::read_dir(&self.config.results.output_dir)
            .await
            .map_err(|error| {
                storage_error(
                    "scan result storage for cleanup",
                    &self.config.results.output_dir,
                    error,
                )
            })?;

        while let Some(owner_entry) = owners.next_entry().await.map_err(|error| {
            storage_error(
                "read result owner during cleanup",
                &self.config.results.output_dir,
                error,
            )
        })? {
            let owner_path = owner_entry.path();
            if !owner_entry
                .file_type()
                .await
                .map_err(|error| storage_error("inspect result owner", &owner_path, error))?
                .is_dir()
            {
                continue;
            }

            let mut files = fs::read_dir(&owner_path)
                .await
                .map_err(|error| storage_error("scan owner results", &owner_path, error))?;
            while let Some(file_entry) = files.next_entry().await.map_err(|error| {
                storage_error("read owner result during cleanup", &owner_path, error)
            })? {
                let file_path = file_entry.path();
                if !file_entry
                    .file_type()
                    .await
                    .map_err(|error| {
                        storage_error("inspect result type during cleanup", &file_path, error)
                    })?
                    .is_file()
                {
                    continue;
                }
                let metadata = file_entry.metadata().await.map_err(|error| {
                    storage_error("inspect result during cleanup", &file_path, error)
                })?;
                if is_expired(&metadata, self.config.results.ttl)? {
                    fs::remove_file(&file_path).await.map_err(|error| {
                        storage_error("remove expired result", &file_path, error)
                    })?;
                    removed_files += 1;
                }
            }

            if fs::read_dir(&owner_path)
                .await
                .map_err(|error| storage_error("rescan result owner", &owner_path, error))?
                .next_entry()
                .await
                .map_err(|error| storage_error("check empty result owner", &owner_path, error))?
                .is_none()
            {
                fs::remove_dir(&owner_path).await.map_err(|error| {
                    storage_error("remove empty result owner", &owner_path, error)
                })?;
                removed_directories += 1;
            }
        }

        tracing::info!(
            removed_files,
            removed_directories,
            "expired result cleanup completed"
        );
        Ok(())
    }

    async fn collect_owner_results(
        &self,
        owner: &str,
        owner_path: &Path,
        results: &mut Vec<StoredResult>,
    ) -> Result<(), AppError> {
        let mut entries = fs::read_dir(owner_path)
            .await
            .map_err(|error| storage_error("scan owner results", owner_path, error))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|error| storage_error("read owner result entry", owner_path, error))?
        {
            let Some(entry) = self.inspect_result_entry(entry).await? else {
                continue;
            };

            results.push(StoredResult {
                owner: owner.to_owned(),
                created_at: OffsetDateTime::from(entry.metadata.modified().map_err(|error| {
                    storage_error("read result modification time", &entry.path, error)
                })?),
                filename: entry.filename,
                path: entry.path,
                size: entry.metadata.len(),
            });
        }

        Ok(())
    }

    async fn inspect_result_entry(
        &self,
        entry: fs::DirEntry,
    ) -> Result<Option<ResultEntry>, AppError> {
        let path = entry.path();
        if !entry
            .file_type()
            .await
            .map_err(|error| storage_error("inspect result file type", &path, error))?
            .is_file()
        {
            return Ok(None);
        }

        let metadata = entry
            .metadata()
            .await
            .map_err(|error| storage_error("inspect result file", &path, error))?;
        if is_expired(&metadata, self.config.results.ttl)? {
            return Ok(None);
        }

        let Some(filename) = entry.file_name().to_str().map(str::to_owned) else {
            tracing::warn!(path = %path.display(), "skipping non-UTF-8 result filename");
            return Ok(None);
        };
        if validate_result_filename(&filename).is_err() {
            return Ok(None);
        }

        Ok(Some(ResultEntry {
            path,
            filename,
            metadata,
        }))
    }
}

fn serialize_students(students: &[PreparedStudent]) -> Result<Vec<u8>, AppError> {
    let mut writer = WriterBuilder::new()
        .terminator(Terminator::Any(b'\n'))
        .from_writer(Vec::new());
    writer
        .write_record([
            "First",
            "Last",
            "Patronymic",
            "Email",
            "Group",
            "Login",
            "Pass",
        ])
        .map_err(|error| ResultError::Csv {
            operation: "write result CSV header",
            source: error,
        })?;

    for student in students {
        let source = &student.identity.source;
        writer
            .write_record([
                source.first_name.as_str(),
                source.last_name.as_str(),
                source.patronymic.as_str(),
                source.email.as_str(),
                source.group.as_str(),
                student.identity.login.as_str(),
                student.password.get(),
            ])
            .map_err(|error| ResultError::Csv {
                operation: "write result CSV row",
                source: error,
            })?;
    }

    writer
        .into_inner()
        .map_err(|error| {
            let source = error.into_error().into();
            ResultError::Csv {
                operation: "finish result CSV",
                source,
            }
        })
        .map_err(Into::into)
}

fn result_filename(created_at: OffsetDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02}_{:02}-{:02}-{:02}_{:09}.csv",
        created_at.year(),
        u8::from(created_at.month()),
        created_at.day(),
        created_at.hour(),
        created_at.minute(),
        created_at.second(),
        created_at.nanosecond(),
    )
}

fn validate_result_filename(filename: &str) -> Result<(), AppError> {
    validate_path_segment(filename, "result filename")?;
    if !filename.ends_with(".csv") {
        tracing::warn!(%filename, "result filename has an invalid extension");
        return Err(AppError::NotFound);
    }
    Ok(())
}

fn validate_path_segment(value: &str, kind: &'static str) -> Result<(), AppError> {
    if value.is_empty()
        || matches!(value, "." | "..")
        || value.contains(['/', '\\'])
        || value.chars().any(char::is_control)
    {
        tracing::warn!(%kind, %value, "invalid result path segment rejected");
        Err(AppError::Validation(format!("invalid {kind}")))
    } else {
        Ok(())
    }
}

fn is_expired(metadata: &std::fs::Metadata, ttl: std::time::Duration) -> Result<bool, AppError> {
    Ok(SystemTime::now()
        .duration_since(metadata.modified().map_err(|error| {
            storage_error(
                "read result modification time",
                Path::new("<metadata>"),
                error,
            )
        })?)
        .is_ok_and(|age| age >= ttl))
}

fn storage_error(operation: &'static str, path: &Path, source: std::io::Error) -> ResultError {
    ResultError::Storage {
        operation,
        path: path.to_owned(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::{net::SocketAddr, path::PathBuf, time::Duration};

    use crate::{
        config::{LdapConfig, ResultConfig},
        entities::import::{PreparedIdentity, SecretString, StudentInput},
    };

    use super::*;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            Self(
                std::env::temp_dir().join(format!("sgu-priemka-result-service-{}", Uuid::new_v4())),
            )
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            if let Err(error) = std::fs::remove_dir_all(&self.0)
                && error.kind() != ErrorKind::NotFound
            {
                panic!("не удалось удалить тестовый каталог: {error}");
            }
        }
    }

    fn service(directory: &TestDirectory, ttl: Duration) -> ResultService {
        let config = Config {
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
                ttl,
            },
            salt: "test-salt".to_owned(),
        };

        ResultService::new(Arc::new(config)).expect("тестовое хранилище должно создаться")
    }

    fn prepared_student() -> PreparedStudent {
        PreparedStudent {
            identity: PreparedIdentity {
                source: StudentInput {
                    source_row: 2,
                    first_name: "Иван, младший".to_owned(),
                    last_name: "Иванов".to_owned(),
                    patronymic: "Иванович".to_owned(),
                    email: "ivan@example.com".to_owned(),
                    group: "001".to_owned(),
                },
                login: "ivanovii".to_owned(),
            },
            password: SecretString::new("secret".to_owned()),
        }
    }

    #[tokio::test]
    async fn creates_reads_and_lists_result_under_ldap_identifier() {
        let directory = TestDirectory::new();
        let service = service(&directory, Duration::from_secs(60));
        let credentials =
            LdapCredentials::new("gadzhiev-mamedovar".to_owned(), "password".to_owned());

        let stored = service
            .create(&credentials, &[prepared_student()])
            .await
            .expect("результат должен сохраниться");

        assert_eq!(stored.owner, "gadzhiev-mamedovar");
        assert_eq!(
            stored.path.parent(),
            Some(directory.0.join("gadzhiev-mamedovar").as_path())
        );

        let bytes = service
            .read(&stored.owner, &stored.filename)
            .await
            .expect("созданный результат должен читаться");
        let csv = String::from_utf8(bytes).expect("результат должен быть UTF-8");
        assert_eq!(
            csv,
            concat!(
                "First,Last,Patronymic,Email,Group,Login,Pass\n",
                "\"Иван, младший\",Иванов,Иванович,ivan@example.com,001,ivanovii,secret\n"
            )
        );

        let listed = service.list().await.expect("список должен читаться");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].owner, "gadzhiev-mamedovar");
        assert_eq!(listed[0].filename, stored.filename);
    }

    #[tokio::test]
    async fn rejects_unsafe_owner_without_creating_external_path() {
        let directory = TestDirectory::new();
        let service = service(&directory, Duration::from_secs(60));
        let credentials = LdapCredentials::new("../admin".to_owned(), "password".to_owned());

        let error = service
            .create(&credentials, &[prepared_student()])
            .await
            .expect_err("небезопасный identifier должен быть отклонён");

        assert!(matches!(error, AppError::Validation(_)));
    }

    #[tokio::test]
    async fn returns_not_found_for_missing_or_unsafe_result() {
        let directory = TestDirectory::new();
        let service = service(&directory, Duration::from_secs(60));

        assert!(matches!(
            service.read("gadzhiev-mamedovar", "missing.csv").await,
            Err(AppError::NotFound)
        ));
        assert!(matches!(
            service.read("gadzhiev-mamedovar", "../secret.csv").await,
            Err(AppError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn removes_expired_results_and_empty_owner_directory() {
        let directory = TestDirectory::new();
        let service = service(&directory, Duration::from_millis(1));
        let credentials =
            LdapCredentials::new("gadzhiev-mamedovar".to_owned(), "password".to_owned());
        let stored = service
            .create(&credentials, &[prepared_student()])
            .await
            .expect("результат должен сохраниться");
        tokio::time::sleep(Duration::from_millis(10)).await;

        service
            .cleanup_expired()
            .await
            .expect("очистка должна завершиться");

        assert!(!stored.path.exists());
        assert!(!directory.0.join("gadzhiev-mamedovar").exists());
        assert!(
            service
                .list()
                .await
                .expect("список должен читаться")
                .is_empty()
        );
    }
}
