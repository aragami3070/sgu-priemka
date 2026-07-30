use uuid::Uuid;

/// Нормализованное представление строки исходного CSV до валидации.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StudentInput {
    /// Номер строки с единицы для ошибок валидации и LDAP.
    pub(crate) source_row: usize,
    /// Фамилия студента из столбца `Last`.
    pub(crate) surname: String,
    /// Имя студента из столбца `First`.
    pub(crate) given_name: String,
    /// Полное ФИО без сокращений из столбца `Fio`.
    pub(crate) full_name: String,
    /// Отчество, извлечённое из полного ФИО для генерации логина.
    pub(crate) patronymic: String,
}

/// Проверенные данные, готовые к поиску конфликтов в LDAP и генерации пароля.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedIdentity {
    /// Исходная нормализованная строка для LDAP-атрибутов и сообщений об ошибках.
    pub(crate) source: StudentInput,
    /// Сгенерированный транслитерированный логин.
    pub(crate) login: String,
}

/// Полностью подготовленная запись студента для создания в LDAP и вывода в итоговый CSV.
#[derive(Clone)]
pub(crate) struct PreparedStudent {
    /// Проверенные исходные данные и сгенерированный логин.
    pub(crate) identity: PreparedIdentity,
    /// Сгенерированный временный пароль.
    pub(crate) password: SecretString,
}

/// Обёртка пароля, исключающая случайный вывод открытого значения через `Debug`.
#[derive(Clone)]
pub(crate) struct SecretString {
    /// Открытое значение закрыто для прямого логирования и сериализации.
    _value: String,
}

impl SecretString {
    /// Возвращает открытое значение только там, где пароль нужно отправить или сериализовать.
    pub(crate) fn expose(&self) -> &str {
        todo!("expose the secret only at the LDAP and CSV boundaries")
    }
}

/// Метаданные запроса, необходимые на протяжении одного импорта.
#[derive(Clone, Debug)]
pub(crate) struct ImportContext {
    /// Идентификатор для подписки на прогресс через WebSocket.
    pub(crate) job_id: String,
    /// `sAMAccountName` пользователя, запустившего импорт.
    pub(crate) username: String,
    /// UUID каталога со сформированными файлами.
    pub(crate) storage_id: Uuid,
    /// Имя загруженного файла для диагностики и аудита.
    pub(crate) original_filename: String,
}
