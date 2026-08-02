use std::sync::Arc;

use uuid::Uuid;

use crate::entities::auth::LdapCredentials;

/// Нормализованное представление строки исходного CSV до валидации.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StudentInput {
    /// Номер строки с единицы для ошибок валидации и LDAP.
    pub(crate) source_row: usize,
    /// Имя студента из столбца `First`.
    pub(crate) first_name: String,
    /// Фамилия студента из столбца `Last`.
    pub(crate) last_name: String,
    /// Отчество студента из столбца `Patronymic`.
    pub(crate) patronymic: String,
    /// Личная контактная почта студента из столбца `Email`.
    pub(crate) email: String,
    /// Учебная группа из столбца `Group`, переносимая в итоговый CSV.
    pub(crate) group: String,
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

/// Обёртка пароля
#[derive(Clone)]
pub(crate) struct SecretString(String);

impl SecretString {
    pub(crate) fn new(password: String) -> Self {
        Self(password)
    }

    pub(crate) fn get(&self) -> &str {
        &self.0
    }
}

/// Метаданные запроса, необходимые на протяжении одного импорта.
#[derive(Clone)]
pub(crate) struct ImportContext {
    /// Идентификатор для подписки на прогресс через WebSocket.
    pub(crate) job_id: String,
    /// `sAMAccountName` пользователя, запустившего импорт.
    pub(crate) username: String,
    /// UUID каталога со сформированными файлами.
    pub(crate) storage_id: Uuid,
    /// Credentials сессии, от имени которой выполняются все LDAP-операции импорта.
    pub(crate) ldap_credentials: Arc<LdapCredentials>,
    /// Имя загруженного файла для диагностики и аудита.
    pub(crate) original_filename: String,
}
