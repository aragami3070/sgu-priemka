use crate::{
    entities::import::{PreparedIdentity, StudentInput},
    errors::ImportError,
};

/// Проверяет ФИО, генерирует логины и отклоняет конфликты внутри файла.
///
/// Проверка конфликтов в LDAP выполняется через `LdapService` после этого чистого этапа.
pub(super) fn validate_students(
    _students: Vec<StudentInput>,
) -> Result<Vec<PreparedIdentity>, ImportError> {
    todo!("validate names, generate logins, and find in-file collisions")
}
