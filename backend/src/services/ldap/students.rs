use crate::{
    entities::{
        import::{PreparedIdentity, PreparedStudent},
        ldap::LdapCollision,
    },
    errors::LdapError,
};

use super::LdapService;

impl LdapService {
    /// Ищет в LDAP значения, сформированные из загруженных строк.
    ///
    /// Операция использует отдельное соединение со служебным bind из конфигурации.
    pub(crate) async fn find_collisions(
        &self,
        _identities: &[PreparedIdentity],
    ) -> Result<Vec<LdapCollision>, LdapError> {
        todo!("bind and search LDAP for login and CN collisions")
    }

    /// Последовательно добавляет пользователя, задаёт пароль и включает учётную запись.
    ///
    /// Операция использует служебный bind, а не credentials пользователя приложения.
    pub(crate) async fn create_user(&self, _student: &PreparedStudent) -> Result<(), LdapError> {
        todo!("add the user, set the password, and enable the account")
    }
}
