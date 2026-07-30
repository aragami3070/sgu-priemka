use crate::{
    entities::import::{SecretString, StudentInput},
    errors::ImportError,
};

/// Формирует логин в нижнем регистре, транслитерируя фамилию и инициалы студента.
pub(super) fn generate_login(_student: &StudentInput) -> Result<String, ImportError> {
    todo!("build and transliterate surname plus initials")
}

/// Вычисляет временный пароль из полного ФИО, UUID строки и серверной соли.
pub(super) fn generate_password(_full_name: &str, _uuid: &str, _salt: &str) -> SecretString {
    todo!("derive the policy-safe SHA-256 password")
}
