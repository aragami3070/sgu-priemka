use std::collections::HashSet;

use crate::{
    entities::import::{PreparedIdentity, StudentInput},
    errors::ImportError,
};

use super::credentials::generate_login;

/// Проверяет ФИО, генерирует логины и отклоняет неизменяемые конфликты внутри файла.
///
/// Проверка конфликтов в входной CSV.
pub(super) fn validate_students(
    students: Vec<StudentInput>,
) -> Result<Vec<PreparedIdentity>, ImportError> {
    let mut full_names = HashSet::with_capacity(students.len());
    let mut prepared = Vec::with_capacity(students.len());

    for student in students {
        let login = generate_login(&student)?;

        let full_name = format!(
            "{} {} {}",
            student.last_name.trim(),
            student.first_name.trim(),
            student.patronymic.trim()
        );

        if !full_names.insert(full_name.to_lowercase()) {
            return Err(ImportError::Collision {
                row: student.source_row,
                attribute: "cn".to_owned(),
            });
        }

        prepared.push(PreparedIdentity {
            source: student,
            login,
        });
    }

    Ok(prepared)
}

#[cfg(test)]
mod tests {
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
            group: "101".to_owned(),
        }
    }

    #[test]
    fn prepares_students_and_preserves_source_data() {
        let students = vec![
            student(2, "Иван", "Иванов", "Иванович"),
            student(3, "Пётр", "Петров", "Петрович"),
        ];

        let prepared = validate_students(students.clone())
            .expect("корректные студенты должны пройти валидацию");

        assert_eq!(prepared.len(), 2);
        assert_eq!(prepared[0].source, students[0]);
        assert_eq!(prepared[0].login, "ivanovii");
        assert_eq!(prepared[1].source, students[1]);
        assert_eq!(prepared[1].login, "petrovpp");
    }

    #[test]
    fn accepts_empty_input() {
        let prepared = validate_students(Vec::new())
            .expect("отсутствие записей не является ошибкой этого этапа");

        assert!(prepared.is_empty());
    }

    #[test]
    fn returns_name_validation_error_from_login_generation() {
        let mut invalid = student(17, "   ", "Иванов", "Иванович");
        invalid.email.clear();

        let error = validate_students(vec![invalid])
            .expect_err("пустое обязательное поле ФИО должно вернуть ошибку");

        assert!(matches!(
            error,
            ImportError::Validation { row: 17, message } if message == "Имя пустое"
        ));
    }

    #[test]
    fn rejects_duplicate_full_name_case_insensitively() {
        let first = student(2, "Иван", "Иванов", "Иванович");
        let second = student(9, "иван", "иванов", "иванович");

        let error = validate_students(vec![first, second])
            .expect_err("одинаковые cn внутри файла должны конфликтовать");

        assert!(matches!(
            error,
            ImportError::Collision { row: 9, attribute } if attribute == "cn"
        ));
    }

    #[test]
    fn rejects_duplicate_generated_login() {
        let first = student(2, "Иван", "Иванов", "Иванович");
        let second = student(11, "Игорь", "Иванов", "Ильич");

        let error = validate_students(vec![first, second])
            .expect_err("одинаковые логины внутри файла должны конфликтовать");

        assert!(matches!(
            error,
            ImportError::Collision { row: 11, attribute }
                if attribute == "sAMAccountName"
        ));
    }

    #[test]
    fn removes_hyphens_and_apostrophes_from_login() {
        let mut valid = student(2, "Аслан-Джан", "Гаджиев-Мамедов'", "Рашидович");
        valid.email.clear();
        valid.group.clear();

        let prepared = validate_students(vec![valid])
            .expect("эти поля не должны отклоняться на текущем этапе");

        assert_eq!(prepared[0].login, "gadzhievmamedovar");
        assert!(prepared[0].source.email.is_empty());
        assert!(prepared[0].source.group.is_empty());
    }
}
