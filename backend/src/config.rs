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
#[derive(Clone)]
pub(crate) struct LdapConfig {
    /// URL LDAP-сервера.
    pub(crate) url: String,
    /// NetBIOS-имя домена для пользовательского bind в формате `DOMAIN\\identifier`.
    pub(crate) user_bind_domain: String,
    /// База поиска учётной записи после успешного пользовательского bind.
    pub(crate) auth_search_base_dn: String,
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
            user_bind_domain: required("LDAP_USER_BIND_DOMAIN")?,
            auth_search_base_dn: required("LDAP_AUTH_SEARCH_BASE_DN")?,
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

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        sync::{Mutex, MutexGuard},
    };

    use super::*;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    const CONFIG_VARIABLES: &[&str] = &[
        "LISTEN_ADDR",
        "COOKIE_SECURE",
        "SESSION_TTL_SECONDS",
        "LDAP_URL",
        "LDAP_USER_BIND_DOMAIN",
        "LDAP_AUTH_SEARCH_BASE_DN",
        "LDAP_USERS_CONTAINER_DN",
        "LDAP_CSIT_ADMINS_GROUP_DN",
        "RESULT_OUTPUT_DIR",
        "RESULT_TTL_SECONDS",
        "PASSWORD_SALT",
    ];

    const REQUIRED_VARIABLES: &[(&str, &str)] = &[
        ("LDAP_URL", "ldap://ldap.test"),
        ("LDAP_USER_BIND_DOMAIN", "MAIN"),
        ("LDAP_AUTH_SEARCH_BASE_DN", "DC=main,DC=sgu,DC=ru"),
        (
            "LDAP_USERS_CONTAINER_DN",
            "OU=groups,OU=КНиИТ,OU=Факультеты,DC=main,DC=sgu,DC=ru",
        ),
        (
            "LDAP_CSIT_ADMINS_GROUP_DN",
            "CN=csit_admins,OU=groups,DC=main,DC=sgu,DC=ru",
        ),
        ("PASSWORD_SALT", "password-salt"),
    ];

    struct TestEnvironment {
        previous: Vec<(&'static str, Option<OsString>)>,
    }

    impl TestEnvironment {
        fn new(overrides: &[(&str, &str)]) -> Self {
            let previous = CONFIG_VARIABLES
                .iter()
                .map(|name| (*name, env::var_os(name)))
                .collect();

            for name in CONFIG_VARIABLES {
                // SAFETY: тест удерживает ENV_LOCK всё время жизни TestEnvironment, поэтому
                // тесты этого модуля не изменяют окружение параллельно.
                unsafe { env::remove_var(name) };
            }
            for (name, value) in REQUIRED_VARIABLES.iter().chain(overrides) {
                // SAFETY: изменение окружения сериализовано тем же ENV_LOCK.
                unsafe { env::set_var(name, value) };
            }

            Self { previous }
        }
    }

    impl Drop for TestEnvironment {
        fn drop(&mut self) {
            for name in CONFIG_VARIABLES {
                // SAFETY: ENV_LOCK всё ещё удерживается и будет освобождён после TestEnvironment.
                unsafe { env::remove_var(name) };
            }
            for (name, value) in &self.previous {
                if let Some(value) = value {
                    // SAFETY: восстановление окружения выполняется под тем же ENV_LOCK.
                    unsafe { env::set_var(name, value) };
                }
            }
        }
    }

    fn lock_environment() -> MutexGuard<'static, ()> {
        ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn config_error(result: Result<Config, ConfigError>) -> ConfigError {
        match result {
            Ok(_) => panic!("ожидалась ошибка конфигурации"),
            Err(error) => error,
        }
    }

    #[test]
    fn loads_all_config_sections() {
        let _lock = lock_environment();
        let _environment = TestEnvironment::new(&[
            ("LISTEN_ADDR", "0.0.0.0:9000"),
            ("COOKIE_SECURE", "false"),
            ("SESSION_TTL_SECONDS", "1800"),
            ("RESULT_OUTPUT_DIR", "/tmp/sgu-priemka-results"),
            ("RESULT_TTL_SECONDS", "7200"),
            ("PASSWORD_SALT", " password salt "),
        ]);

        let config = match Config::from_env() {
            Ok(config) => config,
            Err(error) => panic!("конфигурация должна быть корректной: {error}"),
        };

        assert_eq!(config.listen_addr, SocketAddr::from(([0, 0, 0, 0], 9000)));
        assert!(!config.cookie_secure);
        assert_eq!(config.session_ttl, Duration::from_secs(1800));
        assert_eq!(config.ldap.url, "ldap://ldap.test");
        assert_eq!(config.ldap.user_bind_domain, "MAIN");
        assert_eq!(config.ldap.auth_search_base_dn, "DC=main,DC=sgu,DC=ru");
        assert_eq!(
            config.ldap.users_container_dn,
            "OU=groups,OU=КНиИТ,OU=Факультеты,DC=main,DC=sgu,DC=ru"
        );
        assert_eq!(
            config.ldap.csit_admins_group_dn,
            "CN=csit_admins,OU=groups,DC=main,DC=sgu,DC=ru"
        );
        assert_eq!(
            config.results.output_dir,
            PathBuf::from("/tmp/sgu-priemka-results")
        );
        assert_eq!(config.results.ttl, Duration::from_secs(7200));
        assert_eq!(config.salt, " password salt ");
    }

    #[test]
    fn uses_defaults_for_optional_values() {
        let _lock = lock_environment();
        let _environment = TestEnvironment::new(&[]);

        let config = match Config::from_env() {
            Ok(config) => config,
            Err(error) => panic!("конфигурация должна быть корректной: {error}"),
        };

        assert_eq!(config.listen_addr, SocketAddr::from(([127, 0, 0, 1], 8080)));
        assert!(config.cookie_secure);
        assert_eq!(config.session_ttl, Duration::from_secs(3600));
        assert_eq!(config.results.output_dir, PathBuf::from("output"));
        assert_eq!(config.results.ttl, Duration::from_secs(86400));
    }

    #[test]
    fn reports_missing_required_variable() {
        let _lock = lock_environment();
        let _environment = TestEnvironment::new(&[("LDAP_USERS_CONTAINER_DN", "  ")]);

        let error = config_error(Config::from_env());

        assert!(matches!(
            error,
            ConfigError::Missing("LDAP_USERS_CONTAINER_DN")
        ));
    }

    #[test]
    fn rejects_non_ldap_url() {
        let _lock = lock_environment();
        let _environment = TestEnvironment::new(&[("LDAP_URL", "ldaps://ldap.test")]);

        let error = config_error(Config::from_env());

        assert!(matches!(
            error,
            ConfigError::InvalidValue {
                name: "LDAP_URL",
                ..
            }
        ));
    }

    #[test]
    fn rejects_invalid_scalar_values() {
        let _lock = lock_environment();

        for (name, value) in [
            ("LISTEN_ADDR", "localhost"),
            ("COOKIE_SECURE", "yes"),
            ("SESSION_TTL_SECONDS", "invalid"),
            ("RESULT_TTL_SECONDS", "0"),
        ] {
            let _environment = TestEnvironment::new(&[(name, value)]);
            let error = config_error(Config::from_env());
            assert!(matches!(
                error,
                ConfigError::InvalidValue {
                    name: error_name,
                    ..
                } if error_name == name
            ));
        }
    }
}
