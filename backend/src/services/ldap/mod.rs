//! LDAP-аутентификация операторов и операции с учётными записями студентов.

mod auth;
mod students;

use std::sync::Arc;

use ldap3::{Ldap, LdapConnAsync};

use crate::{
    config::Config, entities::auth::KerberosCredentials, errors::LdapError,
    services::kerberos::KerberosService,
};

/// Единый сервис доступа к LDAP; операции выполняются с credentials текущей сессии.
pub(crate) struct LdapService {
    config: Arc<Config>,
    kerberos: Arc<KerberosService>,
}

impl LdapService {
    /// Создаёт LDAP-сервис из проверенной конфигурации.
    pub(crate) fn new(config: Arc<Config>, kerberos: Arc<KerberosService>) -> Self {
        Self { config, kerberos }
    }

    /// Открывает LDAP-соединение и запускает его driver на Tokio executor.
    async fn connect(&self) -> Result<Ldap, LdapError> {
        let (connection, ldap) = LdapConnAsync::new(&self.config.ldap.url)
            .await
            .map_err(LdapError::connect)?;
        ldap3::drive!(connection);
        Ok(ldap)
    }

    /// Выполняет LDAP-аутентификацию с explicit credential конкретной сессии.
    async fn authenticate_connection(
        &self,
        ldap: &mut Ldap,
        credentials: &KerberosCredentials,
    ) -> Result<(), LdapError> {
        let credential = self.kerberos.gssapi_credential(credentials).await?;
        // `ldap3` самостоятельно добавляет префикс `ldap/`, а `cross-krb5`
        // импортирует результат как Kerberos principal. Realm задаём явно, чтобы
        // service ticket не зависел от системного `default_realm`/`domain_realm`.
        let gssapi_target = format!(
            "{}@{}",
            self.config.ldap.gssapi_host, self.config.kerberos.realm
        );
        let result = ldap
            .sasl_gssapi_cred_bind(credential, &gssapi_target)
            .await
            .map_err(LdapError::authentication_transport)?;
        let result_code = result.rc;

        match result_code {
            0 => Ok(()),
            _ => {
                tracing::warn!(
                    identifier = credentials.identifier(),
                    ldap_result_code = result_code,
                    ldap_diagnostic = %result.text,
                    "LDAP SASL/GSSAPI authentication rejected"
                );
                Err(LdapError::AuthenticationRejected {
                    result_code,
                    diagnostic: result.text,
                })
            }
        }
    }
}
