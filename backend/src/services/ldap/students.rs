use crate::{
    entities::{
        auth::KerberosCredentials,
        import::{PreparedIdentity, PreparedStudent},
        ldap::LdapCollision,
    },
    errors::{LdapError, LdapOperation},
};
use ldap3::{Ldap, Scope, SearchEntry, ldap_escape};
use std::collections::HashSet;

use super::LdapService;

impl LdapService {
    /// Ищет в LDAP значения, сформированные из загруженных строк.
    ///
    /// Операция использует credentials пользователя, запустившего импорт.
    pub(crate) async fn find_collisions(
        &self,
        credentials: &KerberosCredentials,
        identities: &[PreparedIdentity],
    ) -> Result<Vec<LdapCollision>, LdapError> {
        if identities.is_empty() {
            return Ok(Vec::new());
        }

        let mut ldap = self.connect().await?;
        self.authenticate_connection(&mut ldap, credentials).await?;

        let filter = Self::student_login_filter(identities);
        tracing::info!(
            identifier = credentials.identifier(),
            requested_logins = identities.len(),
            "searching LDAP for student login collisions"
        );

        let existing_logins = self
            .search_existing_student_logins(&mut ldap, &filter)
            .await?;
        let collisions = identities
            .iter()
            .filter(|identity| existing_logins.contains(&identity.login.to_lowercase()))
            .map(|identity| LdapCollision {
                source_row: identity.source.source_row,
                attribute: "sAMAccountName".to_owned(),
                value: identity.login.clone(),
            })
            .collect::<Vec<_>>();
        tracing::info!(
            identifier = credentials.identifier(),
            collisions = collisions.len(),
            "LDAP student login collision search completed"
        );
        Ok(collisions)
    }

    /// Формирует безопасный LDAP-фильтр для пакетного поиска логинов студентов.
    fn student_login_filter(identities: &[PreparedIdentity]) -> String {
        let login_filters = identities
            .iter()
            .map(|identity| format!("(sAMAccountName={})", ldap_escape(&identity.login)))
            .collect::<String>();
        format!("(&(objectCategory=person)(objectClass=user)(|{login_filters}))")
    }

    /// Выполняет поиск логинов студентов и нормализует ответ LDAP в set.
    async fn search_existing_student_logins(
        &self,
        ldap: &mut Ldap,
        filter: &str,
    ) -> Result<HashSet<String>, LdapError> {
        let search_result = ldap
            .search(
                &self.config.ldap.users_container_dn,
                Scope::Subtree,
                filter,
                ["sAMAccountName"],
            )
            .await
            .map_err(|error| LdapError::search(LdapOperation::SearchStudent, error))?;
        let (entries, _) = search_result
            .success()
            .map_err(|error| LdapError::search(LdapOperation::SearchStudent, error))?;
        Self::student_logins_from_entries(entries)
    }

    /// Извлекает непустые значения `sAMAccountName` из LDAP-ответа.
    fn student_logins_from_entries(
        entries: Vec<ldap3::ResultEntry>,
    ) -> Result<HashSet<String>, LdapError> {
        let mut existing_logins = HashSet::with_capacity(entries.len());
        for entry in entries {
            let entry = SearchEntry::construct(entry);
            let values = entry
                .attrs
                .into_iter()
                .find_map(|(name, values)| {
                    name.eq_ignore_ascii_case("sAMAccountName")
                        .then_some(values)
                })
                .ok_or(LdapError::MissingAttribute {
                    operation: LdapOperation::SearchStudent,
                    attribute: "sAMAccountName",
                })?;
            existing_logins.extend(
                values
                    .into_iter()
                    .filter(|value| !value.trim().is_empty())
                    .map(|value| value.to_lowercase()),
            );
        }
        Ok(existing_logins)
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
