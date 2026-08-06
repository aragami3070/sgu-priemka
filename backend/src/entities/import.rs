use std::{collections::HashMap, sync::Arc};

use crate::{entities::auth::KerberosCredentials, errors::UnsupportedGroupNumber};

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

/// Одно соответствие номера учебной группы и её названия для LDAP.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Group {
    number: usize,
    name: String,
}

impl Group {
    /// Создаёт соответствие номера и названия группы.
    pub(crate) fn new(number: usize, name: String) -> Self {
        Self { number, name }
    }

    /// Возвращает номер группы.
    #[cfg(test)]
    pub(crate) const fn number(&self) -> usize {
        self.number
    }

    /// Возвращает название группы из TOML.
    #[cfg(test)]
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// Формирует имя LDAP-группы с текущим годом.
    pub(crate) fn ldap_name(&self, year: i32) -> String {
        format!("{} {year}", self.name)
    }
}

/// Загруженный набор соответствий номеров и названий учебных групп.
#[derive(Clone, Debug, Default)]
pub(crate) struct Groups {
    groups: HashMap<usize, Group>,
}

impl Groups {
    /// Создаёт набор групп.
    pub(crate) fn new(groups: HashMap<usize, Group>) -> Self {
        Self { groups }
    }

    /// Ищет группу по номеру из CSV.
    pub(crate) fn get(&self, number: usize) -> Result<&Group, UnsupportedGroupNumber> {
        self.groups
            .get(&number)
            .ok_or(UnsupportedGroupNumber(number))
    }

    /// Возвращает количество загруженных групп.
    pub(crate) fn len(&self) -> usize {
        self.groups.len()
    }
}

/// Проверенные данные, готовые к поиску конфликтов в LDAP и генерации пароля.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedIdentity {
    /// Исходная нормализованная строка для LDAP-атрибутов и сообщений об ошибках.
    pub(crate) source: StudentInput,
    /// Сгенерированный транслитерированный логин.
    pub(crate) login: String,
    /// Проверенное направление, используемое для выбора учебной LDAP-группы.
    pub(crate) group: Group,
}

/// Полностью подготовленная запись студента для создания в LDAP и вывода в итоговый CSV.
#[derive(Clone)]
pub(crate) struct PreparedStudent {
    /// Проверенные исходные данные и сгенерированный логин.
    pub(crate) identity: PreparedIdentity,
    /// Сгенерированный временный пароль.
    pub(crate) password: SecretString,
}

/// Обёртка временного пароля, скрывающая внутреннее строковое представление.
#[derive(Clone)]
pub(crate) struct SecretString(String);

impl SecretString {
    /// Создаёт обёртку для сгенерированного пароля.
    pub(crate) fn new(password: String) -> Self {
        Self(password)
    }

    /// Возвращает пароль для записи в итоговый CSV или LDAP-запрос.
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
    /// Credentials сессии, от имени которой выполняются все LDAP-операции импорта.
    pub(crate) kerberos_credentials: Arc<KerberosCredentials>,
    /// Имя загруженного файла для диагностики и аудита.
    pub(crate) original_filename: String,
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn stores_group_number_and_name() {
        let group = Group::new(151, "ПИ".to_owned());
        assert_eq!(group.number(), 151);
        assert_eq!(group.name(), "ПИ");
        assert_eq!(group.ldap_name(2026), "ПИ 2026");
    }

    #[test]
    fn finds_group_by_number() {
        let groups = Groups::new(HashMap::from([(151, Group::new(151, "ПИ".to_owned()))]));
        assert_eq!(groups.get(151).expect("группа должна найтись").name(), "ПИ");
        assert_eq!(groups.get(101), Err(UnsupportedGroupNumber(101)));
    }
}
