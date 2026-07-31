use std::{env, net::SocketAddr, path::PathBuf, str::FromStr, time::Duration};

use crate::errors::ConfigError;

const DEFAULT_SESSION_TTL_SECONDS: u64 = 60 * 60;
const DEFAULT_RESULT_TTL_SECONDS: u64 = 24 * 60 * 60;
const DEFAULT_RESULT_OUTPUT_DIR: &str = "output";

/// Полная конфигурация приложения, общая для прикладных сервисов.
#[derive(Clone)]
pub(crate) struct Config {
    /// Адрес сокета, на котором HTTP-сервер принимает соединения.
    pub(crate) listen_addr: SocketAddr,
    /// Нужно ли добавлять cookie аутентификации атрибут `Secure` для работы только через HTTPS.
    pub(crate) cookie_secure: bool,
    /// Время жизни локальной аутентифицированной сессии.
    pub(crate) session_ttl: Duration,
    /// Параметры подключения и целевой контейнер LDAP.
    pub(crate) ldap: LdapConfig,
    /// Расположение и срок хранения итоговых CSV-файлов.
    pub(crate) results: ResultConfig,
    /// Серверная соль для вычисления временных паролей студентов.
    pub(crate) salt: String,
}

impl Config {
    /// Загружает `.env`, читает общие настройки и собирает конфигурации подсистем.
    pub(crate) fn load() -> Result<Self, ConfigError> {
        match dotenvy::dotenv() {
            Ok(_) => Ok(()),
            Err(dotenvy::Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(())
            }
            Err(error) => Err(ConfigError::Dotenv(error.to_string())),
        }?;

        Self::from_env()
    }

    /// Собирает конфигурацию из уже загруженных переменных окружения.
    fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            listen_addr: parse_or(
                "LISTEN_ADDR",
                SocketAddr::from(([127, 0, 0, 1], 8080)),
                "адрес в формате IP:PORT",
            )?,
            cookie_secure: parse_or("COOKIE_SECURE", true, "true или false")?,
            session_ttl: duration_or("SESSION_TTL_SECONDS", DEFAULT_SESSION_TTL_SECONDS)?,
            ldap: LdapConfig::load()?,
            results: ResultConfig::load()?,
            salt: required_secret("PASSWORD_SALT")?,
        })
    }
}

/// Настройки подключения к LDAP и целевого контейнера.
///
/// Тип намеренно не реализует `Debug`, потому что содержит пароль служебной учётной записи.
#[derive(Clone)]
pub(crate) struct LdapConfig {
    /// URL LDAP-сервера.
    pub(crate) url: String,
    /// DN-суффикс, который backend добавляет после `CN=<identifier>` при входе.
    pub(crate) user_bind_dn_suffix: String,
    /// База поиска учётной записи после успешного пользовательского bind.
    pub(crate) auth_search_base_dn: String,
    /// Distinguished Name служебной учётной записи для операций со студентами.
    pub(crate) service_bind_dn: String,
    /// Пароль служебной учётной записи LDAP.
    pub(crate) service_bind_password: String,
    /// Distinguished Name контейнера, в котором создаются учётные записи студентов.
    pub(crate) users_container_dn: String,
    /// Distinguished Name группы пользователей, которым разрешён вход.
    pub(crate) csit_admins_group_dn: String,
}

impl LdapConfig {
    /// Читает и проверяет переменные окружения LDAP.
    fn load() -> Result<Self, ConfigError> {
        let url = required("LDAP_URL")?;

        validate_url("LDAP_URL", &url, &["ldap"], "URL со схемой ldap://")?;

        Ok(Self {
            url,
            user_bind_dn_suffix: required("LDAP_USER_BIND_DN_SUFFIX")?,
            auth_search_base_dn: required("LDAP_AUTH_SEARCH_BASE_DN")?,
            service_bind_dn: required("LDAP_SERVICE_BIND_DN")?,
            service_bind_password: required_secret("LDAP_SERVICE_BIND_PASSWORD")?,
            users_container_dn: required("LDAP_USERS_CONTAINER_DN")?,
            csit_admins_group_dn: required("LDAP_CSIT_ADMINS_GROUP_DN")?,
        })
    }
}

/// Настройки файлового хранения итоговых CSV.
#[derive(Clone, Debug)]
pub(crate) struct ResultConfig {
    /// Корневой каталог со сформированными файлами.
    pub(crate) output_dir: PathBuf,
    /// Срок, после которого сформированные файлы можно удалить.
    pub(crate) ttl: Duration,
}

impl ResultConfig {
    /// Читает и проверяет переменные окружения хранилища результатов.
    fn load() -> Result<Self, ConfigError> {
        Ok(Self {
            output_dir: PathBuf::from(optional_or("RESULT_OUTPUT_DIR", DEFAULT_RESULT_OUTPUT_DIR)),
            ttl: duration_or("RESULT_TTL_SECONDS", DEFAULT_RESULT_TTL_SECONDS)?,
        })
    }
}

/// Возвращает обязательную непустую переменную без окружающих пробелов.
fn required(name: &'static str) -> Result<String, ConfigError> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or(ConfigError::Missing(name))
}

/// Возвращает обязательный секрет без изменения его исходного значения.
fn required_secret(name: &'static str) -> Result<String, ConfigError> {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or(ConfigError::Missing(name))
}

/// Возвращает значение переменной или строку по умолчанию.
fn optional_or(name: &str, default: &str) -> String {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_owned())
}

/// Разбирает переменную либо возвращает переданное значение по умолчанию.
fn parse_or<T>(name: &'static str, default: T, expected: &'static str) -> Result<T, ConfigError>
where
    T: FromStr,
{
    let Ok(value) = env::var(name) else {
        return Ok(default);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(default);
    }

    value.parse().map_err(|_| ConfigError::InvalidValue {
        name,
        value: value.to_owned(),
        expected,
    })
}

/// Читает положительное количество секунд и преобразует его в `Duration`.
fn duration_or(name: &'static str, default_seconds: u64) -> Result<Duration, ConfigError> {
    let seconds = parse_or(name, default_seconds, "положительное целое число секунд")?;
    if seconds == 0 {
        return Err(ConfigError::InvalidValue {
            name,
            value: seconds.to_string(),
            expected: "положительное целое число секунд",
        });
    }

    Ok(Duration::from_secs(seconds))
}

/// Проверяет схему и наличие адресной части URL.
fn validate_url(
    name: &'static str,
    value: &str,
    allowed_schemes: &[&str],
    expected: &'static str,
) -> Result<(), ConfigError> {
    let uri = value
        .parse::<http::Uri>()
        .map_err(|_| ConfigError::InvalidValue {
            name,
            value: value.to_owned(),
            expected,
        })?;
    let valid_scheme = uri
        .scheme_str()
        .is_some_and(|scheme| allowed_schemes.contains(&scheme));

    if valid_scheme && uri.authority().is_some() {
        Ok(())
    } else {
        Err(ConfigError::InvalidValue {
            name,
            value: value.to_owned(),
            expected,
        })
    }
}

