//! LDAP-аутентификация операторов и операции с учётными записями студентов.

mod auth;
mod students;

use std::sync::Arc;

use crate::config::LdapConfig;

/// Единый сервис доступа к LDAP с разделёнными пользовательскими и служебными bind.
pub(crate) struct LdapService {
    /// Проверенная конфигурация LDAP и служебной учётной записи.
    config: Arc<LdapConfig>,
}

impl LdapService {
    /// Создаёт LDAP-сервис из проверенной конфигурации.
    pub(crate) fn new(config: Arc<LdapConfig>) -> Self {
        Self { config }
    }
}
