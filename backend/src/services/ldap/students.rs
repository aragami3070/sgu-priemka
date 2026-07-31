use crate::{
    entities::{
        auth::LdapCredentials,
        import::{PreparedIdentity, PreparedStudent},
        ldap::LdapCollision,
    },
    errors::LdapError,
};

use super::LdapService;

impl LdapService {
    /// Ищет в LDAP значения, сформированные из загруженных строк.
    ///
    /// Операция выполняет bind с credentials пользователя, запустившего импорт.
    pub(crate) async fn find_collisions(
        &self,
        _credentials: &LdapCredentials,
        _identities: &[PreparedIdentity],
    ) -> Result<Vec<LdapCollision>, LdapError> {
        todo!("bind and search LDAP for login and CN collisions")
    }

    /// Последовательно добавляет пользователя, задаёт пароль и включает учётную запись.
    ///
    /// Операция выполняет bind с credentials пользователя, запустившего импорт.
    pub(crate) async fn create_user(
        &self,
        _credentials: &LdapCredentials,
        _student: &PreparedStudent,
    ) -> Result<(), LdapError> {
        todo!("add the user, set the password, and enable the account")
    }
}
