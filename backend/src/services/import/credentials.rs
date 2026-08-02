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

/// Вычисляет временный пароль из полного ФИО, UUID строки и серверной соли.
pub(super) fn generate_password(_full_name: &str, _uuid: &str, _salt: &str) -> SecretString {
    todo!("derive the policy-safe SHA-256 password")
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
}
