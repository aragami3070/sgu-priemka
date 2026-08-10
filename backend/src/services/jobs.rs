use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::{Mutex, RwLock, mpsc, watch};
use uuid::Uuid;

use crate::{
    entities::job::{JobStatus, LoginResolutionBatch},
    errors::AppError,
};

const TERMINAL_JOB_TTL: Duration = Duration::from_secs(10 * 60);

/// Состояние одной задачи и канал с её последним статусом.
struct Job {
    owner: String,
    status_sender: watch::Sender<JobStatus>,
    terminal_at: Option<Instant>,
    resolution_sender: mpsc::Sender<LoginResolutionBatch>,
    resolution_receiver: Arc<Mutex<mpsc::Receiver<LoginResolutionBatch>>>,
}

/// Реестр задач импорта и их watch-каналов в памяти.
#[derive(Default)]
pub(crate) struct JobService {
    store: RwLock<HashMap<String, Job>>,
}

impl JobService {
    /// Создаёт пустой реестр задач.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Регистрирует владельца задачи, сохраняет начальный статус и возвращает идентификатор.
    pub(crate) async fn create(
        &self,
        owner: String,
        initial_status: JobStatus,
    ) -> Result<String, AppError> {
        let terminal_at = initial_status.is_terminal().then(Instant::now);
        let (status_sender, _) = watch::channel(initial_status);
        let (resolution_sender, resolution_receiver) = mpsc::channel(8);
        let mut store = self.store.write().await;
        if let Some((job_id, _)) = store
            .iter()
            .find(|(_, job)| job.owner == owner && job.terminal_at.is_none())
        {
            tracing::debug!(%job_id, %owner, "у владельца уже есть активная задача импорта");
            return Err(AppError::ImportBusy);
        }
        let job_id = loop {
            let candidate = Uuid::new_v4().to_string();
            if !store.contains_key(&candidate) {
                break candidate;
            }
        };

        store.insert(
            job_id.clone(),
            Job {
                owner: owner.clone(),
                status_sender,
                terminal_at,
                resolution_sender,
                resolution_receiver: Arc::new(Mutex::new(resolution_receiver)),
            },
        );
        tracing::info!(%job_id, %owner, active_jobs = store.len(), "задача импорта создана");
        Ok(job_id)
    }

    /// Возвращает текущую нетерминальную задачу пользователя для переподключения.
    pub(crate) async fn active_for_owner(&self, owner: &str) -> Option<String> {
        let store = self.store.read().await;
        let active = store
            .iter()
            .find(|(_, job)| job.owner == owner && job.terminal_at.is_none())
            .map(|(job_id, _)| job_id.clone());
        tracing::debug!(
            %owner,
            active_job_id = active.as_deref().unwrap_or("none"),
            "поиск активной задачи импорта завершён"
        );
        active
    }

    /// Передаёт введённую оператором замену ожидающему pipeline-у.
    pub(crate) async fn submit_login_resolutions(
        &self,
        job_id: &str,
        owner: &str,
        resolutions: LoginResolutionBatch,
    ) -> Result<(), AppError> {
        let sender = {
            let store = self.store.read().await;
            let job = store.get(job_id).ok_or(AppError::NotFound)?;
            if job.owner != owner {
                tracing::warn!(%job_id, requested_by = %owner, "доступ к разрешению конфликтов логинов запрещён");
                return Err(AppError::Forbidden);
            }
            job.resolution_sender.clone()
        };

        sender
            .send(resolutions)
            .await
            .map_err(|_| AppError::Internal)?;
        tracing::debug!(%job_id, %owner, "пакет разрешения конфликтов логинов отправлен");
        Ok(())
    }

    /// Ждёт следующую замену логина, отправленную владельцем задачи.
    pub(crate) async fn wait_for_login_resolutions(
        &self,
        job_id: &str,
    ) -> Result<LoginResolutionBatch, AppError> {
        let receiver = {
            let store = self.store.read().await;
            store
                .get(job_id)
                .map(|job| job.resolution_receiver.clone())
                .ok_or(AppError::NotFound)?
        };

        // Реестр job-ов здесь уже не заблокирован: долгое ожидание держит
        // только receiver конкретной задачи и не мешает create/cleanup/publish.
        receiver.lock().await.recv().await.ok_or(AppError::Internal)
    }

    /// Сохраняет последний статус и отправляет его подписчикам.
    pub(crate) async fn publish(&self, job_id: &str, status: JobStatus) -> Result<(), AppError> {
        let mut store = self.store.write().await;
        let job = store.get_mut(job_id).ok_or_else(|| {
            tracing::warn!(%job_id, "нельзя опубликовать статус отсутствующей задачи импорта");
            AppError::NotFound
        })?;

        if job.terminal_at.is_some() {
            tracing::warn!(%job_id, "попытка обновить завершённую задачу импорта отклонена");
            return Err(AppError::Internal);
        }

        if status.is_terminal() {
            job.terminal_at = Some(Instant::now());
        }
        job.status_sender.send_replace(status);
        tracing::debug!(%job_id, terminal = job.terminal_at.is_some(), "статус задачи импорта опубликован");
        Ok(())
    }

    /// Подписывает владельца задачи на текущее и последующие состояния.
    pub(crate) async fn subscribe(
        &self,
        job_id: &str,
        owner: &str,
    ) -> Result<watch::Receiver<JobStatus>, AppError> {
        let store = self.store.read().await;
        let job = store.get(job_id).ok_or(AppError::NotFound)?;
        if job.owner != owner {
            tracing::warn!(%job_id, requested_by = %owner, "подписка на задачу импорта запрещена");
            return Err(AppError::Forbidden);
        }

        tracing::debug!(%job_id, %owner, "подписчик задачи импорта зарегистрирован");
        Ok(job.status_sender.subscribe())
    }

    /// Удаляет terminal-задачи через десять минут после завершения.
    pub(crate) async fn cleanup_expired(&self) {
        tracing::debug!("начинаем очистку просроченных задач импорта");
        let now = Instant::now();
        let mut store = self.store.write().await;
        let before = store.len();
        store.retain(|_, job| {
            job.terminal_at
                .is_none_or(|terminal_at| now.duration_since(terminal_at) < TERMINAL_JOB_TTL)
        });
        tracing::debug!(
            jobs_before = before,
            jobs_after = store.len(),
            removed_jobs = before - store.len(),
            "очистка просроченных задач импорта завершена"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::job::{JobStage, LoginResolution};

    fn progress(current: usize) -> JobStatus {
        JobStatus::Progress {
            stage: JobStage::Parsing,
            current,
            total: 2,
        }
    }

    #[tokio::test]
    async fn subscriber_receives_current_and_new_status() {
        let jobs = JobService::new();
        let job_id = jobs
            .create("admin".to_owned(), progress(0))
            .await
            .expect("job должна создаться");
        let mut receiver = jobs
            .subscribe(&job_id, "admin")
            .await
            .expect("владелец должен подписаться");

        assert!(matches!(
            &*receiver.borrow(),
            JobStatus::Progress { current: 0, .. }
        ));

        jobs.publish(&job_id, progress(2))
            .await
            .expect("статус должен обновиться");
        receiver.changed().await.expect("событие должно прийти");
        assert!(matches!(
            &*receiver.borrow_and_update(),
            JobStatus::Progress { current: 2, .. }
        ));
    }

    #[tokio::test]
    async fn rejects_subscription_from_another_owner() {
        let jobs = JobService::new();
        let job_id = jobs
            .create("admin".to_owned(), progress(0))
            .await
            .expect("job должна создаться");

        assert!(matches!(
            jobs.subscribe(&job_id, "another-admin").await,
            Err(AppError::Forbidden)
        ));
    }

    #[tokio::test]
    async fn terminal_job_cannot_be_updated() {
        let jobs = JobService::new();
        let terminal = JobStatus::Failed {
            stage: JobStage::Parsing,
            code: "parse_error".to_owned(),
            message: "invalid CSV".to_owned(),
            row: Some(2),
        };
        let job_id = jobs
            .create("admin".to_owned(), terminal)
            .await
            .expect("job должна создаться");

        assert!(matches!(
            jobs.publish(&job_id, progress(1)).await,
            Err(AppError::Internal)
        ));
    }

    #[tokio::test]
    async fn owner_can_submit_login_resolution_batch() {
        let jobs = JobService::new();
        let job_id = jobs
            .create("admin".to_owned(), progress(0))
            .await
            .expect("job должна создаться");
        let expected = LoginResolutionBatch {
            resolutions: vec![
                LoginResolution {
                    row: 2,
                    login: "ivanovii".to_owned(),
                    full_name: None,
                },
                LoginResolution {
                    row: 3,
                    login: "ivanovii2".to_owned(),
                    full_name: None,
                },
            ],
        };

        jobs.submit_login_resolutions(&job_id, "admin", expected.clone())
            .await
            .expect("владелец должен отправить замену");

        assert_eq!(
            jobs.wait_for_login_resolutions(&job_id)
                .await
                .expect("pipeline должен получить замену"),
            expected
        );
    }

    #[tokio::test]
    async fn another_owner_cannot_submit_login_resolution_batch() {
        let jobs = JobService::new();
        let job_id = jobs
            .create("admin".to_owned(), progress(0))
            .await
            .expect("job должна создаться");

        assert!(matches!(
            jobs.submit_login_resolutions(
                &job_id,
                "another-admin",
                LoginResolutionBatch {
                    resolutions: vec![LoginResolution {
                        row: 3,
                        login: "ivanovii2".to_owned(),
                        full_name: None,
                    }],
                },
            )
            .await,
            Err(AppError::Forbidden)
        ));
    }

    #[tokio::test]
    async fn waiting_for_resolution_does_not_block_registry_or_allow_second_job_for_owner() {
        let jobs = Arc::new(JobService::new());
        let waiting_job_id = jobs
            .create("admin".to_owned(), progress(0))
            .await
            .expect("первая job должна создаться");
        let waiting_jobs = jobs.clone();
        let waiting_id = waiting_job_id.clone();
        let waiter =
            tokio::spawn(async move { waiting_jobs.wait_for_login_resolutions(&waiting_id).await });
        tokio::task::yield_now().await;

        tokio::time::timeout(
            Duration::from_millis(100),
            jobs.create("admin".to_owned(), progress(0)),
        )
        .await
        .expect("проверка второй job не должна зависать")
        .expect_err("вторая активная job пользователя должна быть запрещена");
        tokio::time::timeout(Duration::from_millis(100), jobs.cleanup_expired())
            .await
            .expect("ожидание ответа не должно блокировать cleanup job-ов");

        jobs.submit_login_resolutions(
            &waiting_job_id,
            "admin",
            LoginResolutionBatch {
                resolutions: vec![LoginResolution {
                    row: 2,
                    login: "ivanovii2".to_owned(),
                    full_name: None,
                }],
            },
        )
        .await
        .expect("ожидающая job должна принять ответ");
        waiter
            .await
            .expect("задача ожидания не должна паниковать")
            .expect("ответ должен дойти до ожидающей job");
    }

    #[tokio::test]
    async fn active_job_is_returned_until_it_becomes_terminal() {
        let jobs = JobService::new();
        let job_id = jobs
            .create("admin".to_owned(), progress(0))
            .await
            .expect("job должна создаться");

        assert_eq!(jobs.active_for_owner("admin").await, Some(job_id.clone()));
        assert_eq!(jobs.active_for_owner("another-admin").await, None);

        jobs.publish(
            &job_id,
            JobStatus::Failed {
                stage: JobStage::Validating,
                code: "cancelled".to_owned(),
                message: "cancelled".to_owned(),
                row: None,
            },
        )
        .await
        .expect("job должна стать terminal");

        assert_eq!(jobs.active_for_owner("admin").await, None);
        jobs.create("admin".to_owned(), progress(0))
            .await
            .expect("после terminal job пользователь может создать новую");
    }
}
