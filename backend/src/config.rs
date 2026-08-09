use std::{env, net::SocketAddr, path::PathBuf, str::FromStr, time::Duration};

use crate::errors::ConfigError;
use crate::services::groups::GroupService;

const DEFAULT_SESSION_TTL_SECONDS: u64 = 60 * 60;
const DEFAULT_RESULT_OUTPUT_DIR: &str = "output";
const DEFAULT_GROUPS_CONFIG_PATH: &str = "groups.toml";
const DEFAULT_SMTP_TIMEOUT_SECONDS: u64 = 30;
const DEFAULT_SMTP_MAX_CONCURRENT: usize = 5;

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
    /// Параметры пользовательских Kerberos credentials.
    pub(crate) kerberos: KerberosConfig,
    /// Расположение итоговых CSV-файлов.
    pub(crate) results: ResultConfig,
    /// Сервис перечитывания соответствий учебных групп из TOML.
    pub(crate) groups: GroupService,
    /// Настройки SMTP и шаблонов почтовой рассылки.
    pub(crate) mail: MailConfig,
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
            kerberos: KerberosConfig::load()?,
            results: ResultConfig::load()?,
            groups: GroupService::new(PathBuf::from(optional_or(
                "GROUPS_CONFIG_PATH",
                DEFAULT_GROUPS_CONFIG_PATH,
            ))),
            mail: MailConfig::load()?,
            salt: required_secret("PASSWORD_SALT")?,
        })
    }
}

/// Режим защищённого соединения с SMTP-сервером.
#[derive(Clone, Copy, Debug)]
pub(crate) enum SmtpSecurity {
    /// Обычное соединение с последующим переходом на TLS через STARTTLS.
    StartTls,
    /// TLS устанавливается сразу при подключении.
    ImplicitTls,
}

/// Настройки SMTP, задаваемые только на стороне backend.
#[derive(Clone, Debug)]
pub(crate) struct MailConfig {
    /// Имя SMTP-сервера.
    pub(crate) smtp_host: String,
    /// Явно заданный порт SMTP или порт по умолчанию для выбранной защиты.
    pub(crate) smtp_port: Option<u16>,
    /// Режим TLS для SMTP.
    pub(crate) smtp_security: SmtpSecurity,
    /// Логин SMTP AUTH.
    pub(crate) smtp_username: Option<String>,
    /// Пароль SMTP AUTH.
    pub(crate) smtp_password: Option<String>,
    /// Адрес отправителя.
    pub(crate) from_address: String,
    /// Отображаемое имя отправителя.
    pub(crate) from_name: String,
    /// Тема писем с credentials.
    pub(crate) subject: String,
    /// Максимальное количество параллельных SMTP-операций.
    pub(crate) max_concurrent: usize,
    /// Таймаут одной SMTP-операции в секундах.
    pub(crate) timeout_seconds: u64,
}

impl MailConfig {
    /// Загружает SMTP-конфигурацию из переменных окружения.
    fn load() -> Result<Self, ConfigError> {
        let smtp_security = match optional_or("SMTP_SECURITY", "starttls")
            .to_ascii_lowercase()
            .as_str()
        {
            "starttls" => SmtpSecurity::StartTls,
            "implicit_tls" | "implicit-tls" | "tls" => SmtpSecurity::ImplicitTls,
            value => {
                return Err(ConfigError::InvalidValue {
                    name: "SMTP_SECURITY",
                    value: value.to_owned(),
                    expected: "starttls или implicit_tls",
                });
            }
        };
        let max_concurrent = parse_or(
            "SMTP_MAX_CONCURRENT",
            DEFAULT_SMTP_MAX_CONCURRENT,
            "положительное целое число",
        )?;
        if max_concurrent == 0 {
            return Err(ConfigError::InvalidValue {
                name: "SMTP_MAX_CONCURRENT",
                value: "0".to_owned(),
                expected: "положительное целое число",
            });
        }
        let timeout_seconds = parse_or(
            "SMTP_TIMEOUT_SECONDS",
            DEFAULT_SMTP_TIMEOUT_SECONDS,
            "положительное целое число секунд",
        )?;
        if timeout_seconds == 0 {
            return Err(ConfigError::InvalidValue {
                name: "SMTP_TIMEOUT_SECONDS",
                value: "0".to_owned(),
                expected: "положительное целое число секунд",
            });
        }
        let smtp_username = optional_env("SMTP_USERNAME");
        let smtp_password = optional_env("SMTP_PASSWORD");
        if smtp_username.is_some() != smtp_password.is_some() {
            return Err(ConfigError::InvalidValue {
                name: "SMTP_USERNAME/SMTP_PASSWORD",
                value: "неполная пара SMTP AUTH credentials".to_owned(),
                expected: "задать обе переменные или не задавать ни одной",
            });
        }
        Ok(Self {
            smtp_host: required("SMTP_HOST")?,
            smtp_port: optional_parse("SMTP_PORT", "порт SMTP")?,
            smtp_security,
            smtp_username,
            smtp_password,
            from_address: required("SMTP_FROM_ADDRESS")?,
            from_name: required("SMTP_FROM_NAME")?,
            subject: required("SMTP_SUBJECT")?,
            max_concurrent,
            timeout_seconds,
        })
    }
}

/// Настройки подключения к LDAP и целевого контейнера.
#[derive(Clone)]
pub(crate) struct LdapConfig {
    /// URL LDAP-сервера.
    pub(crate) url: String,
    /// FQDN LDAP-сервера, из которого GSSAPI строит SPN `ldap/<fqdn>`.
    pub(crate) gssapi_host: String,
    /// База поиска учётной записи после успешной пользовательской аутентификации.
    pub(crate) auth_search_base_dn: String,
    /// Distinguished Name контейнера, в котором создаются учётные записи студентов.
    pub(crate) users_container_dn: String,
    /// Distinguished Name группы пользователей, которым разрешён вход.
    pub(crate) csit_admins_group_dn: String,
    /// Нужно ли принудительно требовать смену временного пароля при первом входе.
    pub(crate) force_password_change: bool,
}

impl LdapConfig {
    /// Читает и проверяет переменные окружения LDAP.
    fn load() -> Result<Self, ConfigError> {
        let url = required("LDAP_URL")?;

        validate_url("LDAP_URL", &url, &["ldap"], "URL со схемой ldap://")?;
        let gssapi_host = required("LDAP_GSSAPI_HOST")?;
        validate_gssapi_host(&gssapi_host, &url)?;

        Ok(Self {
            url,
            gssapi_host,
            auth_search_base_dn: required("LDAP_AUTH_SEARCH_BASE_DN")?,
            users_container_dn: required("LDAP_USERS_CONTAINER_DN")?,
            csit_admins_group_dn: required("LDAP_CSIT_ADMINS_GROUP_DN")?,
            force_password_change: parse_or("LDAP_FORCE_PASSWORD_CHANGE", false, "true или false")?,
        })
    }
}

/// Настройки Kerberos realm и изолированного хранения session credentials.
#[derive(Clone, Debug)]
pub(crate) struct KerberosConfig {
    /// Kerberos realm, добавляемый к нормализованному identifier администратора.
    pub(crate) realm: String,
    /// Закрытый каталог персональных FILE ccache.
    pub(crate) ccache_dir: PathBuf,
}

impl KerberosConfig {
    /// Читает обязательные настройки Kerberos.
    fn load() -> Result<Self, ConfigError> {
        Ok(Self {
            realm: required("KERBEROS_REALM")?,
            ccache_dir: PathBuf::from(required("KERBEROS_CCACHE_DIR")?),
        })
    }
}

/// Настройки файлового хранения итоговых CSV.
#[derive(Clone, Debug)]
pub(crate) struct ResultConfig {
    /// Корневой каталог со сформированными файлами.
    pub(crate) output_dir: PathBuf,
}

impl ResultConfig {
    /// Читает и проверяет переменные окружения хранилища результатов.
    fn load() -> Result<Self, ConfigError> {
        Ok(Self {
            output_dir: PathBuf::from(optional_or("RESULT_OUTPUT_DIR", DEFAULT_RESULT_OUTPUT_DIR)),
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

/// Возвращает непустую переменную окружения без значения по умолчанию.
fn optional_env(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// Разбирает необязательную переменную окружения в число.
fn optional_parse<T>(name: &'static str, expected: &'static str) -> Result<Option<T>, ConfigError>
where
    T: FromStr,
{
    let Some(value) = optional_env(name) else {
        return Ok(None);
    };
    value
        .parse()
        .map(Some)
        .map_err(|_| ConfigError::InvalidValue {
            name,
            value,
            expected,
        })
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

/// Проверяет, что GSSAPI получает FQDN без схемы/порта и тот же host указан в LDAP URL.
fn validate_gssapi_host(host: &str, ldap_url: &str) -> Result<(), ConfigError> {
    let expected = "FQDN без схемы и порта, совпадающий с host в LDAP_URL";
    let authority =
        host.parse::<http::uri::Authority>()
            .map_err(|_| ConfigError::InvalidValue {
                name: "LDAP_GSSAPI_HOST",
                value: host.to_owned(),
                expected,
            })?;
    let is_fqdn = authority.port().is_none()
        && authority.host().contains('.')
        && authority.host().parse::<std::net::IpAddr>().is_err();
    let url_host_matches = ldap_url
        .parse::<http::Uri>()
        .ok()
        .and_then(|uri| uri.host().map(str::to_owned))
        .is_some_and(|url_host| url_host.eq_ignore_ascii_case(authority.host()));

    if is_fqdn && url_host_matches {
        Ok(())
    } else {
        Err(ConfigError::InvalidValue {
            name: "LDAP_GSSAPI_HOST",
            value: host.to_owned(),
            expected,
        })
    }
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
        "LDAP_GSSAPI_HOST",
        "LDAP_AUTH_SEARCH_BASE_DN",
        "LDAP_USERS_CONTAINER_DN",
        "LDAP_CSIT_ADMINS_GROUP_DN",
        "LDAP_FORCE_PASSWORD_CHANGE",
        "KERBEROS_REALM",
        "KERBEROS_CCACHE_DIR",
        "RESULT_OUTPUT_DIR",
        "GROUPS_CONFIG_PATH",
        "SMTP_HOST",
        "SMTP_PORT",
        "SMTP_SECURITY",
        "SMTP_USERNAME",
        "SMTP_PASSWORD",
        "SMTP_FROM_ADDRESS",
        "SMTP_FROM_NAME",
        "SMTP_SUBJECT",
        "SMTP_MAX_CONCURRENT",
        "SMTP_TIMEOUT_SECONDS",
        "PASSWORD_SALT",
    ];

    const REQUIRED_VARIABLES: &[(&str, &str)] = &[
        ("LDAP_URL", "ldap://ldap.test:389"),
        ("LDAP_GSSAPI_HOST", "ldap.test"),
        ("LDAP_AUTH_SEARCH_BASE_DN", "DC=main,DC=sgu,DC=ru"),
        (
            "LDAP_USERS_CONTAINER_DN",
            "OU=groups,OU=КНиИТ,OU=Факультеты,DC=main,DC=sgu,DC=ru",
        ),
        (
            "LDAP_CSIT_ADMINS_GROUP_DN",
            "CN=csit_admins,OU=groups,DC=main,DC=sgu,DC=ru",
        ),
        ("KERBEROS_REALM", "MAIN.SGU.RU"),
        ("KERBEROS_CCACHE_DIR", "/run/ad-provisioner/krb5"),
        ("SMTP_HOST", "smtp.test"),
        ("SMTP_FROM_ADDRESS", "admission@example.com"),
        ("SMTP_FROM_NAME", "Приёмная комиссия"),
        ("SMTP_SUBJECT", "Данные учётной записи"),
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
            ("PASSWORD_SALT", " password salt "),
        ]);

        let config = match Config::from_env() {
            Ok(config) => config,
            Err(error) => panic!("конфигурация должна быть корректной: {error}"),
        };

        assert_eq!(config.listen_addr, SocketAddr::from(([0, 0, 0, 0], 9000)));
        assert!(!config.cookie_secure);
        assert_eq!(config.session_ttl, Duration::from_secs(1800));
        assert_eq!(config.ldap.url, "ldap://ldap.test:389");
        assert_eq!(config.ldap.gssapi_host, "ldap.test");
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
        assert_eq!(config.kerberos.realm, "MAIN.SGU.RU");
        assert_eq!(
            config.kerberos.ccache_dir,
            PathBuf::from("/run/ad-provisioner/krb5")
        );
        assert_eq!(config.salt, " password salt ");
        assert!(!config.ldap.force_password_change);
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
    }

    #[test]
    fn parses_enabled_force_password_change_setting() {
        let _lock = lock_environment();
        let _environment = TestEnvironment::new(&[("LDAP_FORCE_PASSWORD_CHANGE", "true")]);

        let config = Config::from_env().expect("конфигурация должна быть корректной");

        assert!(config.ldap.force_password_change);
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

    #[test]
    fn rejects_gssapi_host_that_does_not_match_ldap_url() {
        let _lock = lock_environment();
        let _environment = TestEnvironment::new(&[("LDAP_GSSAPI_HOST", "other.test")]);

        let error = config_error(Config::from_env());

        assert!(matches!(
            error,
            ConfigError::InvalidValue {
                name: "LDAP_GSSAPI_HOST",
                ..
            }
        ));
    }
}
