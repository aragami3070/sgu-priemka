use ldap3::{Ldap, Scope, SearchEntry, ldap_escape};

use crate::{
    entities::auth::{KerberosCredentials, LdapIdentity},
    errors::{LdapError, LdapOperation},
};

use super::LdapService;

impl LdapService {
    /// Выполняет GSSAPI-аутентификацию и проверяет прямое членство пользователя в `csit_admins`.
    pub(crate) async fn authenticate(
        &self,
        credentials: &KerberosCredentials,
    ) -> Result<LdapIdentity, LdapError> {
        let mut ldap = self.connect().await?;
        self.authenticate_bound(&mut ldap, credentials).await
    }

    /// Выполняет GSSAPI-аутентификацию и загружает разрешённую LDAP-учётную запись.
    async fn authenticate_bound(
        &self,
        ldap: &mut Ldap,
        credentials: &KerberosCredentials,
    ) -> Result<LdapIdentity, LdapError> {
        let identifier = credentials.identifier();
        self.authenticate_connection(ldap, credentials).await?;
        let entry = self.find_authorized_user(ldap, identifier).await?;
        Self::identity_from_entry(entry)
    }

    /// Находит текущего пользователя по `sAMAccountName` и проверяет прямое членство в группе.
    async fn find_authorized_user(
        &self,
        ldap: &mut Ldap,
        identifier: &str,
    ) -> Result<SearchEntry, LdapError> {
        let filter = self.authorization_filter(identifier);
        let search_result = ldap
            .search(
                &self.config.ldap.auth_search_base_dn,
                Scope::Subtree,
                &filter,
                ["sAMAccountName"],
            )
            .await
            .map_err(|error| LdapError::search(LdapOperation::AuthorizeUser, error))?;
        Self::auth_result_check(identifier, search_result)
    }

    /// Классифицирует количество записей, возвращённых поиском авторизации.
    fn auth_result_check(
        _identifier: &str,
        search_result: ldap3::SearchResult,
    ) -> Result<SearchEntry, LdapError> {
        let (mut entries, _) = search_result
            .success()
            .map_err(|error| LdapError::search(LdapOperation::AuthorizeUser, error))?;

        match entries.len() {
            0 => Err(LdapError::Forbidden),
            1 => {
                entries
                    .pop()
                    .map(SearchEntry::construct)
                    .ok_or(LdapError::UnexpectedSearchResult {
                        operation: LdapOperation::AuthorizeUser,
                        expected: "exactly one",
                        actual: 0,
                    })
            }
            actual => Err(LdapError::UnexpectedSearchResult {
                operation: LdapOperation::AuthorizeUser,
                expected: "zero or one",
                actual,
            }),
        }
    }

    /// Собирает LDAP-фильтр, экранируя identifier и DN группы как значения фильтра.
    fn authorization_filter(&self, identifier: &str) -> String {
        format!(
            "(&(objectCategory=person)(objectClass=user)(sAMAccountName={})(memberOf={}))",
            ldap_escape(identifier),
            ldap_escape(&self.config.ldap.csit_admins_group_dn)
        )
    }

    /// Извлекает единственное непустое значение `sAMAccountName`.
    fn identity_from_entry(entry: SearchEntry) -> Result<LdapIdentity, LdapError> {
        let values = entry
            .attrs
            .into_iter()
            .find_map(|(name, values)| {
                name.eq_ignore_ascii_case("sAMAccountName")
                    .then_some(values)
            })
            .ok_or(LdapError::MissingAttribute {
                operation: LdapOperation::AuthorizeUser,
                attribute: "sAMAccountName",
            })?;

        match values.as_slice() {
            [username] if !username.trim().is_empty() => Ok(LdapIdentity {
                username: username.clone(),
            }),
            [] => Err(LdapError::MissingAttribute {
                operation: LdapOperation::AuthorizeUser,
                attribute: "sAMAccountName",
            }),
            _ => Err(LdapError::UnexpectedSearchResult {
                operation: LdapOperation::AuthorizeUser,
                expected: "one non-empty sAMAccountName value",
                actual: values.len(),
            }),
        }
    }
}
