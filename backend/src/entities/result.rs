use std::path::PathBuf;

use time::OffsetDateTime;
use uuid::Uuid;

/// Внутренние метаданные одного сформированного CSV на диске.
#[derive(Clone, Debug)]
pub(crate) struct StoredResult {
    /// UUID каталога, содержащего файл.
    pub(crate) storage_id: Uuid,
    /// Имя файла, сформированное из даты и времени создания.
    pub(crate) filename: String,
    /// Проверенный путь к сформированному файлу.
    pub(crate) path: PathBuf,
    /// Дата и время создания в UTC.
    pub(crate) created_at: OffsetDateTime,
    /// Размер файла в байтах.
    pub(crate) size: u64,
}
