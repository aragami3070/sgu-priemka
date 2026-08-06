use thiserror::Error;

/// Номер группы, для которого в текущей инфраструктуре нет LDAP-отображения.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
#[error("unsupported group number: {0}")]
pub(crate) struct UnsupportedGroupNumber(pub(crate) usize);

/// Ошибки до или во время подготовки строк к добавлению в LDAP.
#[derive(Clone, Debug, Error)]
pub(crate) enum ImportError {
    /// Загруженные байты нельзя декодировать в поддерживаемой кодировке.
    #[error("CSV decoding failed")]
    Decode,
    /// Структуру CSV или значение поля нельзя десериализовать.
    #[error("CSV parsing failed at row {row}: {message}")]
    Parse {
        /// Номер исходной строки с единицы, на которой произошла ошибка разбора.
        row: usize,
        /// Понятное пользователю описание ошибки разбора.
        message: String,
    },
    /// Разобранная строка нарушает правило входных данных или генерации логина.
    #[error("validation failed at row {row}: {message}")]
    Validation {
        /// Номер исходной строки с единицы, на которой не прошла валидация.
        row: usize,
        /// Понятное пользователю описание ошибки валидации.
        message: String,
    },
    /// Номер учебной группы не поддерживается текущим LDAP-отображением.
    #[error("{source} at row {row}")]
    UnsupportedGroup {
        /// Номер исходной строки с единицы.
        row: usize,
        /// Неподдерживаемый числовой идентификатор направления.
        #[source]
        source: UnsupportedGroupNumber,
    },
    /// Сгенерированные данные конфликтуют с другой строкой или записью LDAP.
    #[error("identity collision at row {row}: {attribute}")]
    Collision {
        /// Номер исходной строки с единицы, значение которой уже существует.
        row: usize,
        /// LDAP-атрибут, в котором найден конфликт.
        attribute: String,
    },
}
