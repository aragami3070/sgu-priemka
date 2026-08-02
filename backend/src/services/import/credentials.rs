use sha2::{Digest, Sha256};
use translit::{Gost779B, ToLatin};

use crate::{
    entities::import::{SecretString, StudentInput},
    errors::ImportError,
};

/// Формирует логин в нижнем регистре, транслитерируя фамилию и инициалы студента.
pub(super) fn generate_login(student: &StudentInput) -> Result<String, ImportError> {
    let get_first_char = |s: &str, err_message: String| {
        s.chars().next().ok_or(ImportError::Validation {
            row: student.source_row,
            message: err_message,
        })
    };

    if student.last_name.trim().is_empty() {
        return Err(ImportError::Validation {
            row: student.source_row,
            message: "Фамилия пустая".to_string(),
        });
    }

    Ok(Gost779B::new(translit::Language::Ru)
        .to_latin(&format!(
            "{}{}{}",
            student.last_name.trim(),
            get_first_char(student.first_name.trim(), "Имя пустое".to_string())?,
            get_first_char(student.patronymic.trim(), "Отчество пустое".to_string())?
        ))
        .to_lowercase())
}

/// Вычисляет временный пароль из логина, серверной соли и UUID строки.
pub(super) fn generate_password(login: &str, uuid: &str, salt: &str) -> SecretString {
    let mut hasher = Sha256::new();

    hasher.update(login.as_bytes());
    hasher.update([0]);
    hasher.update(salt.as_bytes());
    hasher.update([0]);
    hasher.update(uuid.as_bytes());

    SecretString::new(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn student(first_name: &str, last_name: &str, patronymic: &str) -> StudentInput {
        StudentInput {
            source_row: 2,
            first_name: first_name.to_owned(),
            last_name: last_name.to_owned(),
            patronymic: patronymic.to_owned(),
            email: "student@example.com".to_owned(),
            group: "101".to_owned(),
        }
    }

    #[test]
    fn generates_login_from_last_name_and_initials() {
        let student = student("Иван", "Иванов", "Иванович");

        let login = generate_login(&student).expect("корректное ФИО должно дать логин");

        assert_eq!(login, "ivanovii");
    }

    #[test]
    fn uses_only_first_letters_of_first_name_and_patronymic() {
        let student = student("Александр", "Петров", "Сергеевич");

        let login = generate_login(&student).expect("корректное ФИО должно дать логин");

        assert_eq!(login, "petrovas");
    }

    #[test]
    fn lowercases_the_complete_transliterated_login() {
        let student = student("ИВАН", "ИВАНОВ", "ИВАНОВИЧ");

        let login = generate_login(&student).expect("корректное ФИО должно дать логин");

        assert_eq!(login, "ivanovii");
    }

    #[test]
    fn transliterates_letters_with_multiple_latin_characters() {
        let student = student("Юрий", "Щукин", "Яковлевич");

        let login = generate_login(&student).expect("корректное ФИО должно дать логин");

        assert_eq!(login, "shhukinyuya");
    }

    #[test]
    fn preserves_hyphen_in_last_name() {
        let student = student("Иван", "Иванов-Петров", "Иванович");

        let login = generate_login(&student).expect("корректное ФИО должно дать логин");

        assert_eq!(login, "ivanov-petrovii");
    }

    #[test]
    fn transliterates_aslan_djan_gadzhiev_mamedov_rashidovich() {
        let student = student("Аслан-Джан", "Гаджиев-Мамедов", "Рашидович");

        let login = generate_login(&student).expect("корректное ФИО должно дать логин");

        assert_eq!(login, "gadzhiev-mamedovar");
    }

    #[test]
    fn personal_email_and_group_do_not_affect_login() {
        let first = student("Иван", "Иванов", "Иванович");
        let mut second = first.clone();
        second.email = "another@example.com".to_owned();
        second.group = "202".to_owned();

        let first_login = generate_login(&first).expect("корректное ФИО должно дать логин");
        let second_login = generate_login(&second).expect("корректное ФИО должно дать логин");

        assert_eq!(first_login, second_login);
    }

    #[test]
    fn returns_validation_error_when_first_name_is_empty() {
        let mut student = student("", "Иванов", "Иванович");
        student.source_row = 17;

        let error = generate_login(&student).expect_err("пустое имя должно вернуть ошибку");

        assert!(matches!(
            error,
            ImportError::Validation { row: 17, message } if message == "Имя пустое"
        ));
    }

    #[test]
    fn returns_validation_error_when_patronymic_is_empty() {
        let mut student = student("Иван", "Иванов", "");
        student.source_row = 23;

        let error = generate_login(&student).expect_err("пустое отчество должно вернуть ошибку");

        assert!(matches!(
            error,
            ImportError::Validation { row: 23, message } if message == "Отчество пустое"
        ));
    }

    #[test]
    fn returns_validation_error_when_last_name_is_empty() {
        let mut student = student("Иван", "", "Иванович");
        student.source_row = 31;

        let error = generate_login(&student).expect_err("пустая фамилия должна вернуть ошибку");

        assert!(matches!(
            error,
            ImportError::Validation { row: 31, message } if message == "Фамилия пустая"
        ));
    }

    #[test]
    fn returns_validation_error_when_first_name_contains_only_whitespace() {
        let mut student = student(" \t\n ", "Иванов", "Иванович");
        student.source_row = 37;

        let error = generate_login(&student)
            .expect_err("имя только из пробельных символов должно вернуть ошибку");

        assert!(matches!(
            error,
            ImportError::Validation { row: 37, message } if message == "Имя пустое"
        ));
    }

    #[test]
    fn returns_validation_error_when_last_name_contains_only_whitespace() {
        let mut student = student("Иван", " \t\n ", "Иванович");
        student.source_row = 41;

        let error = generate_login(&student)
            .expect_err("фамилия только из пробельных символов должна вернуть ошибку");

        assert!(matches!(
            error,
            ImportError::Validation { row: 41, message } if message == "Фамилия пустая"
        ));
    }

    #[test]
    fn returns_validation_error_when_patronymic_contains_only_whitespace() {
        let mut student = student("Иван", "Иванов", " \t\n ");
        student.source_row = 43;

        let error = generate_login(&student)
            .expect_err("отчество только из пробельных символов должно вернуть ошибку");

        assert!(matches!(
            error,
            ImportError::Validation { row: 43, message } if message == "Отчество пустое"
        ));
    }

    #[test]
    fn trims_surrounding_whitespace_before_generating_login() {
        let student = student(" \tИван\n", "  Иванов  ", "\nИванович\t");

        let login =
            generate_login(&student).expect("ФИО с пробелами по краям должно быть допустимо");

        assert_eq!(login, "ivanovii");
    }
}
