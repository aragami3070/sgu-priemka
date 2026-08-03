use std::{io, path::PathBuf};

use thiserror::Error;

/// Ошибки формирования и хранения итоговых CSV-файлов.
#[derive(Debug, Error)]
pub(crate) enum ResultError {
    /// CSV не удалось сериализовать.
    #[error("result CSV operation `{operation}` failed: {source}")]
    Csv {
        /// Операция, завершившаяся ошибкой.
        operation: &'static str,
        /// Ошибка библиотеки CSV.
        #[source]
        source: csv::Error,
    },
    /// Операция с файловым хранилищем завершилась ошибкой.
    #[error("result storage operation `{operation}` failed for `{path}`: {source}")]
    Storage {
        /// Операция, завершившаяся ошибкой.
        operation: &'static str,
        /// Путь, с которым выполнялась операция.
        path: PathBuf,
        /// Ошибка файловой системы.
        #[source]
        source: io::Error,
    },
}
