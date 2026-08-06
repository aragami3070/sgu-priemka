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
        let value: toml::Value = toml::from_str(&text)?;
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
