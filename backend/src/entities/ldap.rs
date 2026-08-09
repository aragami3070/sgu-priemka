use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct UpnRepairEntry {
    pub(crate) dn: String,
    #[serde(rename = "sAMAccountName")]
    pub(crate) sam_account_name: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct UpnRepairResult {
    #[serde(rename = "sAMAccountName")]
    pub(crate) sam_account_name: String,
    #[serde(rename = "userPrincipalName")]
    pub(crate) user_principal_name: String,
}

/// Существующий LDAP-атрибут, конфликтующий с одной входной строкой.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LdapCollision {
    /// Номер строки CSV с конфликтующим студентом, начиная с единицы.
    pub(crate) source_row: usize,
    /// LDAP-атрибут, в котором уже существует сгенерированное значение.
    pub(crate) attribute: String,
    /// Конфликтующее значение, например логин или полное ФИО.
    pub(crate) value: String,
}
