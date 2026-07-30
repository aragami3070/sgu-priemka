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
