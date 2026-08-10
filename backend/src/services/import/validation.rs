use std::collections::HashMap;

use crate::{
    entities::import::{Groups, PreparedIdentity, StudentInput},
    errors::ImportError,
};

use super::credentials::generate_login;

/// Проверяет ФИО, генерирует логины и отклоняет неизменяемые конфликты внутри файла.
///
/// Проверка конфликтов в входной CSV.
pub(super) fn validate_students(
    students: Vec<StudentInput>,
    groups: &Groups,
) -> Result<Vec<PreparedIdentity>, ImportError> {
    let mut prepared = Vec::with_capacity(students.len());

    for student in students {
        let login = generate_login(&student)?;
        let group_number = student
            .group
            .parse::<usize>()
            .map_err(|_| ImportError::Validation {
                row: student.source_row,
                message: "Номер группы должен быть целым положительным числом".to_owned(),
            })?;
        let group =
            groups
                .get(group_number)
                .cloned()
                .map_err(|source| ImportError::UnsupportedGroup {
                    row: student.source_row,
                    source,
                })?;

        prepared.push(PreparedIdentity {
            source: student,
            login,
            group,
        });
    }

    Ok(prepared)
}

/// Возвращает строки с дубликатами логина или полного имени внутри CSV.
pub(super) fn find_identity_collisions(identities: &[PreparedIdentity]) -> Vec<usize> {
    let mut login_counts = HashMap::with_capacity(identities.len());
    let mut full_name_counts = HashMap::with_capacity(identities.len());
    for identity in identities {
        *login_counts
            .entry(identity.login.to_ascii_lowercase())
            .or_insert(0usize) += 1;
        *full_name_counts
            .entry(identity.full_name().to_lowercase())
            .or_insert(0usize) += 1;
    }

    identities
        .iter()
        .enumerate()
        .filter_map(|(index, identity)| {
            let login = identity.login.to_ascii_lowercase();
            let full_name = identity.full_name().to_lowercase();
            (login_counts[&login] > 1 || full_name_counts[&full_name] > 1).then_some(index)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::{entities::import::Group, errors::UnsupportedGroupNumber};

    use super::*;

    fn student(
        source_row: usize,
        first_name: &str,
        last_name: &str,
        patronymic: &str,
    ) -> StudentInput {
        StudentInput {
            source_row,
            first_name: first_name.to_owned(),
            last_name: last_name.to_owned(),
            patronymic: patronymic.to_owned(),
            email: format!("student{source_row}@example.com"),
            group: "151".to_owned(),
        }
    }

    fn groups() -> Groups {
        Groups::new(HashMap::from([(151, Group::new(151, "ПИ".to_owned()))]))
    }

    #[test]
    fn prepares_students_and_preserves_source_data() {
        let students = vec![
            student(2, "Иван", "Иванов", "Иванович"),
            student(3, "Пётр", "Петров", "Петрович"),
        ];

        let prepared = validate_students(students.clone(), &groups())
            .expect("корректные студенты должны пройти валидацию");

        assert_eq!(prepared.len(), 2);
        assert_eq!(prepared[0].source, students[0]);
        assert_eq!(prepared[0].login, "ivanovii");
        assert_eq!(prepared[0].group.name(), "ПИ");
        assert_eq!(prepared[1].source, students[1]);
        assert_eq!(prepared[1].login, "petrovpp");
        assert_eq!(prepared[1].group.name(), "ПИ");
    }

    #[test]
    fn accepts_empty_input() {
        let prepared = validate_students(Vec::new(), &groups())
            .expect("отсутствие записей не является ошибкой этого этапа");

        assert!(prepared.is_empty());
    }

    #[test]
    fn returns_name_validation_error_from_login_generation() {
        let mut invalid = student(17, "   ", "Иванов", "Иванович");
        invalid.email.clear();

        let error = validate_students(vec![invalid], &groups())
            .expect_err("пустое обязательное поле ФИО должно вернуть ошибку");

        assert!(matches!(
            error,
            ImportError::Validation { row: 17, message } if message == "Имя пустое"
        ));
    }

    #[test]
    fn reports_duplicate_full_name_for_interactive_resolution() {
        let first = student(2, "Иван", "Иванов", "Иванович");
        let second = student(9, "иван", "иванов", "иванович");

        let prepared = validate_students(vec![first, second], &groups())
            .expect("дубликат полного имени должен передаваться в интерактивное разрешение");

        assert_eq!(find_identity_collisions(&prepared), vec![0, 1]);
    }

    #[test]
    fn reports_duplicate_generated_login_for_interactive_resolution() {
        let first = student(2, "Иван", "Иванов", "Иванович");
        let second = student(11, "Игорь", "Иванов", "Ильич");

        let prepared = validate_students(vec![first, second], &groups())
            .expect("исправляемый конфликт логинов не должен останавливать валидацию");

        assert_eq!(find_identity_collisions(&prepared), vec![0, 1]);
        assert_eq!(prepared[0].source.source_row, 2);
        assert_eq!(prepared[1].source.source_row, 11);
    }

    #[test]
    fn removes_hyphens_and_apostrophes_from_login() {
        let mut valid = student(2, "Аслан-Джан", "Гаджиев-Мамедов'", "Рашидович");
        valid.email.clear();

        let prepared = validate_students(vec![valid], &groups())
            .expect("ФИО с разделителями и поддерживаемая группа должны пройти проверку");

        assert_eq!(prepared[0].login, "gadzhievmamedovar");
        assert!(prepared[0].source.email.is_empty());
    }

    #[test]
    fn preserves_group_with_leading_zeroes_and_maps_its_numeric_value() {
        let mut valid = student(4, "Иван", "Иванов", "Иванович");
        valid.group = "00151".to_owned();

        let prepared = validate_students(vec![valid], &groups())
            .expect("ведущие нули не должны менять номер направления");

        assert_eq!(prepared[0].source.group, "00151");
        assert_eq!(prepared[0].group.name(), "ПИ");
    }

    #[test]
    fn rejects_non_numeric_group_at_source_row() {
        let mut invalid = student(27, "Иван", "Иванов", "Иванович");
        invalid.group = "ПИ".to_owned();

        let error = validate_students(vec![invalid], &groups())
            .expect_err("текстовое название группы не должно приниматься");

        assert!(matches!(
            error,
            ImportError::Validation { row: 27, message }
                if message == "Номер группы должен быть целым положительным числом"
        ));
    }

    #[test]
    fn rejects_unsupported_group_at_source_row() {
        let mut invalid = student(31, "Иван", "Иванов", "Иванович");
        invalid.group = "101".to_owned();

        let error = validate_students(vec![invalid], &groups())
            .expect_err("неподдерживаемая группа должна вернуть отдельную ошибку");

        assert!(matches!(
            error,
            ImportError::UnsupportedGroup {
                row: 31,
                source: UnsupportedGroupNumber(101)
            }
        ));
    }
}
