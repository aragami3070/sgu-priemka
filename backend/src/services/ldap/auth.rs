use crate::{entities::auth::LdapIdentity, errors::LdapAuthError};

use super::LdapService;

impl LdapService {
    /// Строит пользовательский Bind DN, проверяет bind и членство в `csit_admins`.
    ///
    /// Предварительный формат DN: `CN=<escaped identifier>,<configured suffix>`.
    /// Точный суффикс и допустимость почты должны быть подтверждены инфраструктурой.
    /// Пользовательское соединение создаётся только на время вызова, а связанная запись
    /// ищется в настроенной базе и возвращает канонический `sAMAccountName`.
    pub(crate) async fn authenticate(
        &self,
        _identifier: &str,
        _password: &str,
    ) -> Result<LdapIdentity, LdapAuthError> {
        todo!("construct the bind DN, bind as the user, and verify csit_admins membership")
    }
}
