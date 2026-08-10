use base64::{Engine as _, engine::general_purpose::STANDARD};

use crate::{
    entities::{
        auth::KerberosCredentials,
        import::{PreparedIdentity, PreparedStudent},
        ldap::LdapCollision,
    },
    errors::{LdapError, LdapOperation, LdapPhase},
};
use ldap3::{Ldap, LdapResult, Mod, Scope, SearchEntry, ldap_escape};
use std::collections::HashSet;
use time::OffsetDateTime;

use super::LdapService;

/// Код LDAP для объекта, которого уже нет в каталоге.
const LDAP_NO_SUCH_OBJECT: u32 = 32;

impl LdapService {
    /// Исправляет userPrincipalName для записей JSON-отчёта от имени сессии.
    pub(crate) async fn repair_user_principal_names(
        &self,
        credentials: &KerberosCredentials,
        entries: &[crate::entities::ldap::UpnRepairEntry],
    ) -> Result<Vec<crate::entities::ldap::UpnRepairResult>, LdapError> {
        if entries.is_empty() {
            return Ok(Vec::new());
        }

        let mut ldap = self.connect().await?;
        self.authenticate_connection(&mut ldap, credentials).await?;
        let mut repaired = Vec::with_capacity(entries.len());

        for entry in entries {
            let login = entry.sam_account_name.trim();
            if login.is_empty() {
                return Err(LdapError::Operation {
                    phase: LdapPhase::RepairUserPrincipalName,
                    possibly_created: false,
                    message: "sAMAccountName is empty".to_owned(),
                });
            }
            let user_dn = decode_report_dn(&entry.dn).ok_or(LdapError::Operation {
                phase: LdapPhase::RepairUserPrincipalName,
                possibly_created: false,
                message: "invalid distinguishedName in JSON report".to_owned(),
            })?;
            let user_principal_name = format!("{login}@main.sgu.ru");
            let result = ldap
                .modify(
                    &user_dn,
                    vec![Mod::Replace(
                        b"userPrincipalName".to_vec(),
                        HashSet::from([user_principal_name.as_bytes().to_vec()]),
                    )],
                )
                .await
                .map_err(|error| {
                    Self::operation_error(LdapPhase::RepairUserPrincipalName, false, error)
                })?;
            Self::check_operation(result, LdapPhase::RepairUserPrincipalName, false)?;
            tracing::debug!(
                identifier = credentials.identifier(),
                login = %login,
                user_dn = %user_dn,
                user_principal_name = %user_principal_name,
                "userPrincipalName исправлен"
            );
            repaired.push(crate::entities::ldap::UpnRepairResult {
                sam_account_name: login.to_owned(),
                user_principal_name,
            });
        }
        Ok(repaired)
    }

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
        tracing::debug!(
            identifier = credentials.identifier(),
            requested_logins = identities.len(),
            "поиск конфликтов логинов студентов в LDAP"
        );

        let existing_logins = self
            .search_existing_student_logins(&mut ldap, &filter)
            .await?;
        let name_filter = Self::student_full_name_filter(identities);
        let existing_full_names = self
            .search_existing_student_full_names(&mut ldap, &name_filter)
            .await?;
        let collisions = identities
            .iter()
            .flat_map(|identity| {
                let mut collisions = Vec::new();
                if existing_logins.contains(&identity.login.to_lowercase()) {
                    collisions.push(LdapCollision {
                        source_row: identity.source.source_row,
                        attribute: "sAMAccountName".to_owned(),
                        value: identity.login.clone(),
                    });
                }
                let full_name = identity.full_name();
                if existing_full_names.contains(&full_name.to_lowercase()) {
                    collisions.push(LdapCollision {
                        source_row: identity.source.source_row,
                        attribute: "cn".to_owned(),
                        value: full_name,
                    });
                }
                collisions
            })
            .collect::<Vec<_>>();
        tracing::debug!(
            identifier = credentials.identifier(),
            collisions = collisions.len(),
            "поиск конфликтов логинов студентов в LDAP завершён"
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

    /// Формирует безопасный LDAP-фильтр для пакетного поиска полных имён.
    fn student_full_name_filter(identities: &[PreparedIdentity]) -> String {
        let filters = identities
            .iter()
            .map(|identity| format!("(cn={})", ldap_escape(identity.full_name())))
            .collect::<String>();
        format!("(&(objectCategory=person)(objectClass=user)(|{filters}))")
    }

    /// Выполняет поиск логинов студентов и нормализует ответ LDAP в set.
    async fn search_existing_student_logins(
        &self,
        ldap: &mut Ldap,
        filter: &str,
    ) -> Result<HashSet<String>, LdapError> {
        let search_result = ldap
            .search(
                &self.config.ldap.auth_search_base_dn,
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

    /// Выполняет поиск существующих полных имён в контейнере пользователей.
    async fn search_existing_student_full_names(
        &self,
        ldap: &mut Ldap,
        filter: &str,
    ) -> Result<HashSet<String>, LdapError> {
        let search_result = ldap
            .search(
                &self.config.ldap.users_container_dn,
                Scope::Subtree,
                filter,
                ["cn"],
            )
            .await
            .map_err(|error| LdapError::search(LdapOperation::SearchStudent, error))?;
        let (entries, _) = search_result
            .success()
            .map_err(|error| LdapError::search(LdapOperation::SearchStudent, error))?;
        Self::student_full_names_from_entries(entries)
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

    /// Извлекает непустые значения `cn` из LDAP-ответа.
    fn student_full_names_from_entries(
        entries: Vec<ldap3::ResultEntry>,
    ) -> Result<HashSet<String>, LdapError> {
        let mut existing_names = HashSet::with_capacity(entries.len());
        for entry in entries {
            let entry = SearchEntry::construct(entry);
            let values = entry
                .attrs
                .into_iter()
                .find_map(|(name, values)| name.eq_ignore_ascii_case("cn").then_some(values))
                .ok_or(LdapError::MissingAttribute {
                    operation: LdapOperation::SearchStudent,
                    attribute: "cn",
                })?;
            existing_names.extend(
                values
                    .into_iter()
                    .filter(|value| !value.trim().is_empty())
                    .map(|value| value.to_lowercase()),
            );
        }
        Ok(existing_names)
    }

    /// Последовательно добавляет пользователя, задаёт пароль и включает учётную запись.
    ///
    /// Операция использует credentials пользователя, запустившего импорт.
    pub(crate) async fn create_user(
        &self,
        credentials: &KerberosCredentials,
        student: &PreparedStudent,
    ) -> Result<(), LdapError> {
        let (user_dn, group_dn, group_name) = self.student_dns(student)?;
        let mut ldap = self.connect().await?;
        self.authenticate_connection(&mut ldap, credentials).await?;

        self.ensure_group(&mut ldap, &group_dn, &group_name).await?;

        tracing::debug!(
            identifier = credentials.identifier(),
            login = %student.identity.login,
            user_dn = %user_dn,
            "создание отключённой учётной записи студента в LDAP"
        );
        let result = ldap
            .add(&user_dn, Self::student_attributes(student))
            .await
            .map_err(|error| Self::operation_error(LdapPhase::AddObject, false, error))?;
        Self::check_operation(result, LdapPhase::AddObject, false)?;

        tracing::debug!(
            identifier = credentials.identifier(),
            login = %student.identity.login,
            "объект студента в LDAP создан и отключён"
        );
        let result = ldap
            .modify(
                &user_dn,
                vec![Mod::Replace(
                    b"unicodePwd".to_vec(),
                    HashSet::from([encode_ad_password(student.password.get())]),
                )],
            )
            .await
            .map_err(|error| Self::operation_error(LdapPhase::SetPassword, true, error))?;
        Self::check_operation(result, LdapPhase::SetPassword, true)?;
        tracing::debug!(
            identifier = credentials.identifier(),
            login = %student.identity.login,
            "пароль студента в LDAP установлен"
        );

        if self.config.ldap.force_password_change {
            // Требуем сменить временный пароль при первом входе только при
            // явно включённой настройке LDAP_FORCE_PASSWORD_CHANGE.
            let result = ldap
                .modify(
                    &user_dn,
                    vec![Mod::Replace(
                        b"pwdLastSet".to_vec(),
                        HashSet::from([b"0".to_vec()]),
                    )],
                )
                .await
                .map_err(|error| {
                    Self::operation_error(LdapPhase::RequirePasswordChange, true, error)
                })?;

            Self::check_operation(result, LdapPhase::RequirePasswordChange, true)?;
            tracing::debug!(
                identifier = credentials.identifier(),
                login = %student.identity.login,
                "обязательная смена пароля при первом входе включена"
            );
        } else {
            tracing::debug!(
                identifier = credentials.identifier(),
                login = %student.identity.login,
                "обязательная смена пароля при первом входе отключена настройкой LDAP"
            );
        }

        let result = ldap
            .modify(
                &user_dn,
                vec![Mod::Replace(
                    b"userAccountControl".to_vec(),
                    HashSet::from([b"512".to_vec()]),
                )],
            )
            .await
            .map_err(|error| Self::operation_error(LdapPhase::EnableAccount, true, error))?;
        Self::check_operation(result, LdapPhase::EnableAccount, true)?;
        tracing::debug!(
            identifier = credentials.identifier(),
            login = %student.identity.login,
            "учётная запись студента в LDAP включена"
        );

        let result = ldap
            .modify(
                &group_dn,
                vec![Mod::Add(
                    b"member".to_vec(),
                    HashSet::from([user_dn.as_bytes().to_vec()]),
                )],
            )
            .await
            .map_err(|error| Self::operation_error(LdapPhase::AddToGroup, true, error))?;
        Self::check_operation(result, LdapPhase::AddToGroup, true)?;
        tracing::debug!(
            identifier = credentials.identifier(),
            login = %student.identity.login,
            group_dn = %group_dn,
            "студент добавлен в LDAP-группу"
        );
        Ok(())
    }

    /// Удаляет одну учётную запись студента из LDAP.
    ///
    /// DN восстанавливается из ФИО и настроенного контейнера пользователей,
    /// поэтому credentials текущей сессии используются только для подключения
    /// и выполнения операции от имени вошедшего администратора.
    pub(crate) async fn delete_user(
        &self,
        credentials: &KerberosCredentials,
        student: &PreparedStudent,
    ) -> Result<(), LdapError> {
        let (user_dn, _, _) = self.student_dns(student)?;
        let mut ldap = self.connect().await?;
        self.authenticate_connection(&mut ldap, credentials).await?;

        tracing::debug!(
            identifier = credentials.identifier(),
            login = %student.identity.login,
            user_dn = %user_dn,
            "удаление учётной записи студента из LDAP"
        );
        let result = ldap
            .delete(&user_dn)
            .await
            .map_err(|error| Self::operation_error(LdapPhase::DeleteObject, false, error))?;
        if result.rc == LDAP_NO_SUCH_OBJECT {
            tracing::debug!(
                identifier = credentials.identifier(),
                login = %student.identity.login,
                user_dn = %user_dn,
                "учётная запись студента уже отсутствует, удаление пропущено"
            );
            return Ok(());
        }
        Self::check_operation(result, LdapPhase::DeleteObject, false)?;

        tracing::debug!(
            identifier = credentials.identifier(),
            login = %student.identity.login,
            user_dn = %user_dn,
            "учётная запись студента удалена из LDAP"
        );
        Ok(())
    }

    /// Формирует DN пользователя и учебной группы для текущего года.
    fn student_dns(
        &self,
        student: &PreparedStudent,
    ) -> Result<(String, String, String), LdapError> {
        let full_name = student.identity.full_name();
        validate_dn_value(&full_name, "full name")?;
        validate_dn_value(&student.identity.login, "sAMAccountName")?;
        let group_name = student
            .identity
            .group
            .ldap_name(OffsetDateTime::now_utc().year());
        validate_dn_value(&group_name, "group name")?;
        let users_container = &self.config.ldap.users_container_dn;
        Ok((
            format!("CN={full_name},{users_container}"),
            format!("CN={group_name},{users_container}"),
            group_name,
        ))
    }

    /// Создаёт учебную группу, если в контейнере ещё нет ровно одной такой группы.
    async fn ensure_group(
        &self,
        ldap: &mut Ldap,
        group_dn: &str,
        group_name: &str,
    ) -> Result<(), LdapError> {
        let filter = format!("(&(objectCategory=group)(cn={}))", ldap_escape(group_name));
        let search_result = ldap
            .search(
                &self.config.ldap.users_container_dn,
                Scope::Subtree,
                &filter,
                ["distinguishedName"],
            )
            .await
            .map_err(|error| LdapError::search(LdapOperation::EnsureGroup, error))?;
        let (entries, _) = search_result
            .success()
            .map_err(|error| LdapError::search(LdapOperation::EnsureGroup, error))?;
        match entries.len() {
            1 => Ok(()),
            0 => {
                tracing::debug!(%group_dn, "LDAP-группа отсутствует, создаём её");
                let result = ldap
                    .add(
                        group_dn,
                        vec![
                            (
                                b"objectClass".to_vec(),
                                HashSet::from([b"top".to_vec(), b"group".to_vec()]),
                            ),
                            (
                                b"cn".to_vec(),
                                HashSet::from([group_name.as_bytes().to_vec()]),
                            ),
                            (
                                b"sAMAccountName".to_vec(),
                                HashSet::from([group_name.as_bytes().to_vec()]),
                            ),
                            (
                                b"groupType".to_vec(),
                                HashSet::from([b"-2147483646".to_vec()]),
                            ),
                        ],
                    )
                    .await
                    .map_err(|error| Self::operation_error(LdapPhase::AddObject, false, error))?;
                Self::check_operation(result, LdapPhase::AddObject, false)
            }
            actual => Err(LdapError::UnexpectedSearchResult {
                operation: LdapOperation::EnsureGroup,
                expected: "zero or one",
                actual,
            }),
        }
    }

    /// Формирует атрибуты выключенной учётной записи Active Directory.
    fn student_attributes(student: &PreparedStudent) -> Vec<(Vec<u8>, HashSet<Vec<u8>>)> {
        let source = &student.identity.source;
        let full_name = student.identity.full_name();
        let principal = format!("{}@main.sgu.ru", student.identity.login);
        vec![
            (
                b"objectClass".to_vec(),
                HashSet::from([
                    b"top".to_vec(),
                    b"person".to_vec(),
                    b"organizationalPerson".to_vec(),
                    b"user".to_vec(),
                ]),
            ),
            (
                b"cn".to_vec(),
                HashSet::from([full_name.as_bytes().to_vec()]),
            ),
            (
                b"displayName".to_vec(),
                HashSet::from([full_name.as_bytes().to_vec()]),
            ),
            (
                b"sn".to_vec(),
                HashSet::from([source.last_name.as_bytes().to_vec()]),
            ),
            (
                b"givenName".to_vec(),
                HashSet::from([source.first_name.as_bytes().to_vec()]),
            ),
            (
                b"sAMAccountName".to_vec(),
                HashSet::from([student.identity.login.as_bytes().to_vec()]),
            ),
            (
                b"userPrincipalName".to_vec(),
                HashSet::from([principal.as_bytes().to_vec()]),
            ),
            (
                b"userAccountControl".to_vec(),
                HashSet::from([b"514".to_vec()]),
            ),
        ]
    }

    /// Преобразует transport-ошибку LDAP в ошибку конкретного этапа создания.
    fn operation_error(
        phase: LdapPhase,
        possibly_created: bool,
        error: ldap3::LdapError,
    ) -> LdapError {
        LdapError::Operation {
            phase,
            possibly_created,
            message: error.to_string(),
        }
    }

    /// Проверяет result code ответа LDAP-операции.
    fn check_operation(
        result: LdapResult,
        phase: LdapPhase,
        possibly_created: bool,
    ) -> Result<(), LdapError> {
        if result.rc == 0 {
            Ok(())
        } else {
            Err(LdapError::Operation {
                phase,
                possibly_created,
                message: format!("LDAP result code {}: {}", result.rc, result.text),
            })
        }
    }
}

/// Кодирует пароль Active Directory в quoted UTF-16LE для `unicodePwd`.
fn encode_ad_password(password: &str) -> Vec<u8> {
    format!("\"{password}\"")
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect()
}

/// Проверяет, что значение можно безопасно использовать как компонент DN.
fn validate_dn_value(value: &str, field: &'static str) -> Result<(), LdapError> {
    if value.is_empty()
        || value.starts_with(' ')
        || value.ends_with(' ')
        || value.chars().any(|character| {
            matches!(
                character,
                ',' | '=' | '+' | '<' | '>' | '#' | ';' | '"' | '\\'
            )
        })
    {
        return Err(LdapError::Operation {
            phase: LdapPhase::AddObject,
            possibly_created: false,
            message: format!("invalid {field} for LDAP distinguished name"),
        });
    }
    Ok(())
}

/// Декодирует DN из текущего JSON-отчёта или принимает обычный DN.
fn decode_report_dn(value: &str) -> Option<String> {
    if !value.contains(',')
        && let Ok(decoded) = STANDARD.decode(value)
        && let Ok(decoded) = String::from_utf8(decoded)
        && decoded.contains('=')
    {
        return Some(decoded);
    }
    (!value.trim().is_empty() && value.contains('=')).then(|| value.to_owned())
}
