use crate::{
    entities::{
        auth::KerberosCredentials,
        import::{PreparedIdentity, PreparedStudent},
        ldap::LdapCollision,
    },
    errors::LdapError,
};

use super::LdapService;

impl LdapService {
    /// Ищет в LDAP значения, сформированные из загруженных строк.
    ///
    /// Операция использует credentials пользователя, запустившего импорт.
    pub(crate) async fn find_collisions(
        &self,
        _credentials: &KerberosCredentials,
        _identities: &[PreparedIdentity],
    ) -> Result<Vec<LdapCollision>, LdapError> {
        todo!("search LDAP for login collisions")
    }

    /// Последовательно добавляет пользователя, задаёт пароль и включает учётную запись.
    ///
    /// Операция использует credentials пользователя, запустившего импорт.
    pub(crate) async fn create_user(
        &self,
        _credentials: &KerberosCredentials,
        _student: &PreparedStudent,
    ) -> Result<(), LdapError> {
        todo!("add the user, set the password, and enable the account")
    }
}
