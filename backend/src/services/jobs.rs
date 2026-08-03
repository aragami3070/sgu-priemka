use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use tokio::sync::{RwLock, watch};
use uuid::Uuid;

use crate::{entities::job::JobStatus, errors::AppError};

const TERMINAL_JOB_TTL: Duration = Duration::from_secs(10 * 60);

/// Состояние одной задачи и канал с её последним статусом.
struct Job {
    owner: String,
    status_sender: watch::Sender<JobStatus>,
    terminal_at: Option<Instant>,
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
        let mut store = self.store.write().await;
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
            },
        );
        tracing::info!(%job_id, %owner, active_jobs = store.len(), "import job created");
        Ok(job_id)
    }

    /// Сохраняет последний статус и отправляет его подписчикам.
    pub(crate) async fn publish(&self, job_id: &str, status: JobStatus) -> Result<(), AppError> {
        let mut store = self.store.write().await;
        let job = store.get_mut(job_id).ok_or_else(|| {
            tracing::warn!(%job_id, "cannot publish status for missing import job");
            AppError::NotFound
        })?;

        if job.terminal_at.is_some() {
            tracing::warn!(%job_id, "attempt to update terminal import job rejected");
            return Err(AppError::Internal);
        }

        if status.is_terminal() {
            job.terminal_at = Some(Instant::now());
        }
        job.status_sender.send_replace(status);
        tracing::info!(%job_id, terminal = job.terminal_at.is_some(), "import job status published");
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
            tracing::warn!(%job_id, requested_by = %owner, "import job subscription denied");
            return Err(AppError::Forbidden);
        }

        tracing::info!(%job_id, %owner, "import job subscriber registered");
        Ok(job.status_sender.subscribe())
    }

    /// Удаляет terminal-задачи через десять минут после завершения.
    pub(crate) async fn cleanup_expired(&self) {
        let now = Instant::now();
        let mut store = self.store.write().await;
        let before = store.len();
        store.retain(|_, job| {
            job.terminal_at
                .is_none_or(|terminal_at| now.duration_since(terminal_at) < TERMINAL_JOB_TTL)
        });
        tracing::info!(
            jobs_before = before,
            jobs_after = store.len(),
            removed_jobs = before - store.len(),
            "expired import job cleanup completed"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::job::JobStage;

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
}
