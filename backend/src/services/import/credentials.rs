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
    Ok(Gost779B::new(translit::Language::Ru)
        .to_latin(&format!(
            "{}{}{}",
            student.last_name,
            get_first_char(&student.first_name, "Имя пустое".to_string())?,
            get_first_char(&student.patronymic, "Отчество пустое".to_string())?
        ))
        .to_lowercase())
}

/// Вычисляет временный пароль из полного ФИО, UUID строки и серверной соли.
pub(super) fn generate_password(_full_name: &str, _uuid: &str, _salt: &str) -> SecretString {
    todo!("derive the policy-safe SHA-256 password")
}
