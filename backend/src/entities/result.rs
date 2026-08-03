use std::path::PathBuf;

use time::OffsetDateTime;

/// Внутренние метаданные одного сформированного CSV на диске.
#[derive(Clone, Debug)]
pub(crate) struct StoredResult {
    /// `sAMAccountName` владельца каталога, содержащего файл.
    pub(crate) owner: String,
    /// Имя файла, сформированное из даты и времени создания.
    pub(crate) filename: String,
    /// Проверенный путь к сформированному файлу.
    pub(crate) path: PathBuf,
    /// Дата и время создания в UTC.
    pub(crate) created_at: OffsetDateTime,
    /// Размер файла в байтах.
    pub(crate) size: u64,
}
