use std::{collections::HashMap, fs, path::PathBuf};

use crate::entities::import::{Group, Groups};
use crate::errors::GroupServiceError;

/// Сервис перечитывания соответствий номеров и названий учебных групп.
#[derive(Clone, Debug)]
pub(crate) struct GroupService {
    path: PathBuf,
}

impl GroupService {
    /// Создаёт сервис для указанного TOML-файла.
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Каждый вызов заново читает TOML-файл, поэтому изменения видны без перезапуска.
    pub(crate) fn reload(&self) -> Result<Groups, GroupServiceError> {
        let text = fs::read_to_string(&self.path).map_err(|source| GroupServiceError::Read {
            path: self.path.clone(),
            source,
        })?;
        Self::parse_text(&text)
    }

    /// Проверяет новый TOML и атомарно заменяет текущий файл групп.
    pub(crate) fn replace(&self, bytes: &[u8]) -> Result<Groups, GroupServiceError> {
        let text = std::str::from_utf8(bytes).map_err(|_| GroupServiceError::InvalidUtf8)?;
        let groups = Self::parse_text(text)?;
        let temporary_path = self
            .path
            .with_extension(format!("toml-{}.tmp", uuid::Uuid::new_v4()));
        fs::write(&temporary_path, bytes).map_err(|source| GroupServiceError::Read {
            path: temporary_path.clone(),
            source,
        })?;
        if let Err(source) = fs::rename(&temporary_path, &self.path) {
            let _ = fs::remove_file(&temporary_path);
            return Err(GroupServiceError::Read {
                path: self.path.clone(),
                source,
            });
        }
        Ok(groups)
    }

    /// Разбирает TOML-текст в набор групп.
    fn parse_text(text: &str) -> Result<Groups, GroupServiceError> {
        let value: toml::Value = toml::from_str(text)?;
        let table = value
            .get("groups")
            .and_then(toml::Value::as_table)
            .or_else(|| value.as_table())
            .ok_or(GroupServiceError::InvalidRoot)?;

        let mut groups = HashMap::with_capacity(table.len());
        for (number, value) in table {
            let number = number
                .parse::<usize>()
                .map_err(|_| GroupServiceError::InvalidNumber(number.clone()))?;
            let name = value
                .as_str()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .ok_or(GroupServiceError::EmptyName(number))?
                .to_owned();
            groups.insert(number, Group::new(number, name));
        }
        Ok(Groups::new(groups))
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::*;

    fn temporary_path() -> PathBuf {
        std::env::temp_dir().join(format!("sgu-priemka-groups-{}.toml", uuid::Uuid::new_v4()))
    }

    #[test]
    fn reloads_groups_from_toml() {
        let path = temporary_path();
        fs::write(&path, "[groups]\n151 = \"ПИ\"\n").expect("TOML должен записаться");

        let service = GroupService::new(path.clone());
        let groups = service.reload().expect("TOML должен разобраться");
        assert_eq!(groups.get(151).expect("группа должна найтись").name(), "ПИ");

        fs::write(&path, "[groups]\n151 = \"Программная инженерия\"\n")
            .expect("обновлённый TOML должен записаться");
        let groups = service
            .reload()
            .expect("обновлённый TOML должен разобраться");
        assert_eq!(
            groups.get(151).expect("группа должна найтись").name(),
            "Программная инженерия"
        );

        fs::remove_file(path).expect("временный TOML должен удалиться");
    }
}
