use ldap3::{Ldap, LdapConnAsync, Scope, SearchEntry, ldap_escape};

use crate::{entities::auth::LdapIdentity, errors::LdapAuthError};

use super::LdapService;

impl LdapService {
    /// Строит пользовательское bind-имя, проверяет bind и членство в `csit_admins`.
    ///
    /// Формат bind-имени: `<configured domain>\\<identifier>`.
    /// Пользовательское соединение создаётся только на время вызова, а связанная запись
    /// ищется в настроенной базе и возвращает канонический `sAMAccountName`.
    pub(crate) async fn authenticate(
        &self,
        identifier: &str,
        password: &str,
    ) -> Result<LdapIdentity, LdapAuthError> {
        tracing::info!(
            identifier,
            password_present = !password.is_empty(),
            "LDAP authentication started"
        );
        let identifier = Self::validate_credentials(identifier, password)?;
        let bind_name = self.build_bind_name(identifier);
        let mut ldap = self.connect().await?;

        let authentication = self
            .authenticate_bound(&mut ldap, &bind_name, identifier, password)
            .await;
        match &authentication {
            Ok(identity) => tracing::info!(
                username = %identity.username,
                "LDAP authentication stages completed successfully"
            ),
            Err(error) => tracing::warn!(%error, "LDAP authentication failed"),
        }
        Self::unbind(&mut ldap).await;

        authentication
    }

    /// Проверяет обязательные поля и возвращает identifier без окружающих пробелов.
    fn validate_credentials<'a>(
        identifier: &'a str,
        password: &str,
    ) -> Result<&'a str, LdapAuthError> {
        let identifier = identifier.trim();
        let password = password.trim();
        if identifier.is_empty() || password.is_empty() {
            tracing::info!(
                identifier_empty = identifier.is_empty(),
                password_empty = password.is_empty(),
                "LDAP credentials validation failed"
            );
            Err(LdapAuthError::InvalidCredentials)
        } else {
            tracing::info!(identifier, "LDAP credentials passed basic validation");
            Ok(identifier)
        }
    }

    /// Формирует down-level logon name в формате `DOMAIN\\identifier`.
    fn build_bind_name(&self, identifier: &str) -> String {
        let bind_name = format!("{}\\{}", self.config.ldap.user_bind_domain, identifier);
        tracing::info!(identifier, %bind_name, "LDAP bind name constructed");
        bind_name
    }

    /// Открывает LDAP-соединение и запускает его обработчик на Tokio executor.
    async fn connect(&self) -> Result<Ldap, LdapAuthError> {
        tracing::info!("connecting to LDAP");
        let (connection, ldap) =
            LdapConnAsync::new(&self.config.ldap.url)
                .await
                .map_err(|error| {
                    tracing::warn!(%error, "failed to connect to LDAP for authentication");
                    LdapAuthError::Unavailable
                })?;
        ldap3::drive!(connection);
        tracing::info!(ldap_url = %self.config.ldap.url, "LDAP connection established and driver spawned");

        Ok(ldap)
    }

    /// Выполняет bind и загружает разрешённую LDAP-учётную запись.
    async fn authenticate_bound(
        &self,
        ldap: &mut Ldap,
        bind_name: &str,
        identifier: &str,
        password: &str,
    ) -> Result<LdapIdentity, LdapAuthError> {
        Self::bind_user(ldap, bind_name, password).await?;
        tracing::info!(%bind_name, "LDAP bind stage completed");
        let entry = self.find_authorized_user(ldap, identifier).await?;
        tracing::info!(entry_dn = %entry.dn, "LDAP authorization search stage completed");
        let identity = Self::identity_from_entry(entry)?;
        tracing::info!(username = %identity.username, "LDAP identity extraction completed");
        Ok(identity)
    }

    /// Выполняет простой пользовательский bind и классифицирует LDAP-код результата.
    async fn bind_user(
        ldap: &mut Ldap,
        bind_name: &str,
        password: &str,
    ) -> Result<(), LdapAuthError> {
        tracing::info!(%bind_name, password_present = !password.is_empty(), "sending LDAP simple bind");
        let result = ldap
            .simple_bind(bind_name, password)
            .await
            .map_err(|error| {
                tracing::warn!(%error, "LDAP bind request failed");
                LdapAuthError::Unavailable
            })?;
        let result_code = result.rc;
        tracing::info!(
            %bind_name,
            ldap_result_code = result_code,
            ldap_matched_dn = %result.matched,
            ldap_diagnostic_text = %result.text,
            referrals = ?result.refs,
            "LDAP simple bind response received"
        );

        if result.success().is_ok() {
            tracing::info!(%bind_name, "LDAP simple bind accepted");
            return Ok(());
        }

        if result_code == 49 {
            tracing::info!(%bind_name, "LDAP simple bind rejected credentials");
            Err(LdapAuthError::InvalidCredentials)
        } else {
            tracing::warn!(
                ldap_result_code = result_code,
                "LDAP rejected the authentication bind"
            );
            Err(LdapAuthError::Unavailable)
        }
    }

    /// Находит текущего пользователя по `sAMAccountName` и проверяет прямое членство в группе.
    async fn find_authorized_user(
        &self,
        ldap: &mut Ldap,
        identifier: &str,
    ) -> Result<SearchEntry, LdapAuthError> {
        let filter = self.authorization_filter(identifier);
        tracing::info!("sending LDAP authorization search");
        let search_result = ldap
            .search(
                &self.config.ldap.auth_search_base_dn,
                Scope::Subtree,
                &filter,
                ["sAMAccountName"],
            )
            .await
            .map_err(|error| {
                tracing::warn!(%error, "LDAP authorization search failed");
                LdapAuthError::Unavailable
            })?;
        Self::auth_result_check(identifier, search_result)
    }

    /// Определяет имеет ли этот пользователь доступ по ответу ldap
    fn auth_result_check(
        identifier: &str,
        search_result: ldap3::SearchResult,
    ) -> Result<SearchEntry, LdapAuthError> {
        let (mut entries, _) = search_result.success().map_err(|error| {
            tracing::warn!(%error, "LDAP rejected the authorization search");
            LdapAuthError::Unavailable
        })?;

        match entries.len() {
            0 => {
                tracing::info!(
                    identifier,
                    "LDAP user is not a direct member of the allowed group"
                );
                Err(LdapAuthError::Forbidden)
            }
            1 => {
                tracing::info!(
                    identifier,
                    "LDAP authorization search returned exactly one user"
                );
                entries
                    .pop()
                    .map(SearchEntry::construct)
                    .ok_or(LdapAuthError::UnexpectedSearchResult)
            }
            count => {
                tracing::warn!(count, "LDAP authorization search returned multiple users");
                Err(LdapAuthError::UnexpectedSearchResult)
            }
        }
    }

    /// Собирает LDAP-фильтр, экранируя identifier и DN группы как значения фильтра.
    fn authorization_filter(&self, identifier: &str) -> String {
        let filter = format!(
            "(&(objectCategory=person)(objectClass=user)(sAMAccountName={})(memberOf={}))",
            ldap_escape(identifier),
            ldap_escape(&self.config.ldap.csit_admins_group_dn)
        );
        tracing::info!(
            identifier,
            allowed_group_dn = %self.config.ldap.csit_admins_group_dn,
            %filter,
            "LDAP authorization filter constructed"
        );
        filter
    }

    /// Извлекает единственное непустое значение `sAMAccountName`.
    fn identity_from_entry(entry: SearchEntry) -> Result<LdapIdentity, LdapAuthError> {
        tracing::info!(
            entry_dn = %entry.dn,
            attribute_names = ?entry.attrs.keys().collect::<Vec<_>>(),
            "extracting identity from LDAP search entry"
        );
        let values = entry
            .attrs
            .into_iter()
            .find_map(|(name, values)| {
                name.eq_ignore_ascii_case("sAMAccountName")
                    .then_some(values)
            })
            .ok_or(LdapAuthError::MissingSamAccountName)?;
        tracing::info!(value_count = values.len(), "sAMAccountName attribute found");

        match values.as_slice() {
            [username] if !username.trim().is_empty() => {
                tracing::info!(%username, "valid sAMAccountName extracted");
                Ok(LdapIdentity {
                    username: username.clone(),
                })
            }
            [] => {
                tracing::warn!("sAMAccountName contains no values");
                Err(LdapAuthError::MissingSamAccountName)
            }
            _ => {
                tracing::warn!(
                    value_count = values.len(),
                    "sAMAccountName contains an unexpected number of values"
                );
                Err(LdapAuthError::UnexpectedSearchResult)
            }
        }
    }

    /// Закрывает пользовательское LDAP-соединение, не изменяя основной результат входа.
    async fn unbind(ldap: &mut Ldap) {
        tracing::info!("sending LDAP unbind request");
        if let Err(error) = ldap.unbind().await {
            tracing::warn!(%error, "failed to unbind LDAP authentication connection");
        } else {
            tracing::info!("LDAP authentication connection unbound successfully");
        }
    }
}
