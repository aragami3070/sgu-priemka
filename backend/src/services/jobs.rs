use tokio::sync::watch;

use crate::{entities::job::JobStatus, errors::AppError};

/// Реестр задач импорта и их watch-каналов в памяти.
#[derive(Default)]
pub(crate) struct JobService;

impl JobService {
    /// Создаёт пустой реестр задач.
    pub(crate) fn new() -> Self {
        todo!("initialize the in-memory job store")
    }

    /// Регистрирует владельца задачи, сохраняет начальный статус и возвращает идентификатор.
    pub(crate) async fn create(
        &self,
        _owner: String,
        _initial_status: JobStatus,
    ) -> Result<String, AppError> {
        todo!("create a job and its watch channel")
    }

    /// Сохраняет последний статус и отправляет его подписчикам.
    pub(crate) async fn publish(&self, _job_id: &str, _status: JobStatus) -> Result<(), AppError> {
        todo!("publish and retain the latest job status")
    }

    /// Подписывает владельца задачи на текущее и последующие состояния.
    pub(crate) async fn subscribe(
        &self,
        _job_id: &str,
        _owner: &str,
    ) -> Result<watch::Receiver<JobStatus>, AppError> {
        todo!("authorize the job owner and subscribe to updates")
    }

    /// Удаляет завершённые задачи после истечения срока хранения.
    pub(crate) async fn cleanup_expired(&self) {
        todo!("remove expired terminal jobs")
    }
}
