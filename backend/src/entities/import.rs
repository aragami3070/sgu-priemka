use std::sync::Arc;

use crate::{entities::auth::LdapCredentials, errors::UnsupportedGroupNumber};

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

/// Поддерживаемое направление обучения, определяемое номером группы из CSV.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Group {
    /// Фундаментальная информатика и информационные технологии, номер 111.
    Phiit,
    /// Информатика и вычислительная техника, номер 121.
    Ivt,
    /// Компьютерная безопасность, номер 131.
    Kb,
    /// Математическое обеспечение и администрирование информационных систем, номер 141.
    Moais,
    /// Программная инженерия, номер 151.
    Pi,
    /// Педагогическое образование, номер 161.
    Po,
    /// Системный анализ и управление, номер 181.
    Sau,
}

impl TryFrom<usize> for Group {
    type Error = UnsupportedGroupNumber;

    fn try_from(group_number: usize) -> Result<Self, Self::Error> {
        match group_number {
            111 => Ok(Self::Phiit),
            121 => Ok(Self::Ivt),
            131 => Ok(Self::Kb),
            141 => Ok(Self::Moais),
            151 => Ok(Self::Pi),
            161 => Ok(Self::Po),
            181 => Ok(Self::Sau),
            _ => Err(UnsupportedGroupNumber(group_number)),
        }
    }
}

impl Group {
    /// Возвращает номер группы из входного CSV.
    pub(crate) const fn number(self) -> usize {
        match self {
            Self::Phiit => 111,
            Self::Ivt => 121,
            Self::Kb => 131,
            Self::Moais => 141,
            Self::Pi => 151,
            Self::Po => 161,
            Self::Sau => 181,
        }
    }

    /// Возвращает русское сокращение направления для имени LDAP-группы.
    pub(crate) const fn russian_name(self) -> &'static str {
        match self {
            Self::Phiit => "ФИИТ",
            Self::Ivt => "ИВТ",
            Self::Kb => "КБ",
            Self::Moais => "МОАИС",
            Self::Pi => "ПИ",
            Self::Po => "ПО",
            Self::Sau => "САУ",
        }
    }

    /// Формирует принятое в LDAP имя группы с одним пробелом перед годом.
    pub(crate) fn ldap_name(self, year: i32) -> String {
        format!("{} {year}", self.russian_name())
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
    /// Credentials сессии, от имени которой выполняются все LDAP-операции импорта.
    pub(crate) ldap_credentials: Arc<LdapCredentials>,
    /// Имя загруженного файла для диагностики и аудита.
    pub(crate) original_filename: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_every_supported_number_to_expected_group() {
        let cases = [
            (111, Group::Phiit, "ФИИТ"),
            (121, Group::Ivt, "ИВТ"),
            (131, Group::Kb, "КБ"),
            (141, Group::Moais, "МОАИС"),
            (151, Group::Pi, "ПИ"),
            (161, Group::Po, "ПО"),
            (181, Group::Sau, "САУ"),
        ];

        for (number, expected, expected_name) in cases {
            let group = Group::try_from(number).expect("номер должен поддерживаться");
            assert_eq!(group, expected);
            assert_eq!(group.number(), number);
            assert_eq!(group.russian_name(), expected_name);
        }
    }

    #[test]
    fn rejects_unsupported_group_number() {
        let error = Group::try_from(101).expect_err("номер не должен поддерживаться");

        assert_eq!(error, UnsupportedGroupNumber(101));
    }

    #[test]
    fn formats_ldap_name_with_exactly_one_space_before_year() {
        assert_eq!(Group::Pi.ldap_name(2026), "ПИ 2026");
        assert_eq!(Group::Phiit.ldap_name(2030), "ФИИТ 2030");
    }
}
