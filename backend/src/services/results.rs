use uuid::Uuid;

use crate::{
    config::ResultConfig,
    entities::{import::PreparedStudent, result::StoredResult},
    errors::AppError,
};

/// Файловый сервис для сформированных CSV с учётными данными.
pub(crate) struct ResultService;

impl ResultService {
    /// Создаёт при необходимости выходной каталог и подготавливает хранилище.
    pub(crate) fn new(_config: &ResultConfig) -> Result<Self, AppError> {
        todo!("initialize and validate the result directory")
    }

    /// Атомарно записывает CSV `Fio,Login,Pass` под именем из текущей даты и времени.
    pub(crate) async fn create(
        &self,
        _storage_id: Uuid,
        _students: &[PreparedStudent],
    ) -> Result<StoredResult, AppError> {
        todo!("atomically write a date-time-named Fio,Login,Pass CSV")
    }

    /// Возвращает все неистёкшие результаты в порядке отображения.
    pub(crate) async fn list(&self) -> Result<Vec<StoredResult>, AppError> {
        todo!("scan and sort non-expired CSV results")
    }

    /// Читает результат после проверки UUID каталога и имени файла.
    pub(crate) async fn read(
        &self,
        _storage_id: Uuid,
        _filename: &str,
    ) -> Result<Vec<u8>, AppError> {
        todo!("validate the generated path and read the CSV result")
    }

    /// Удаляет результаты старше настроенного срока хранения.
    pub(crate) async fn cleanup_expired(&self) -> Result<(), AppError> {
        todo!("delete expired result files and empty directories")
    }
}
