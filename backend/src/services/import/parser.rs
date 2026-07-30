use crate::{entities::import::StudentInput, errors::ImportError};

/// Декодирует загруженный CSV и преобразует столбцы `First`, `Last` и `Fio` во входные записи.
pub(super) fn parse_csv(_bytes: &[u8]) -> Result<Vec<StudentInput>, ImportError> {
    todo!("decode UTF-8/CP1251 and parse First,Last,Fio records")
}
