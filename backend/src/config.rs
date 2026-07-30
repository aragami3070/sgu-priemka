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
       todo!()
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

/// Настройки файлового хранения итоговых CSV.
#[derive(Clone, Debug)]
pub(crate) struct ResultConfig {
    /// Корневой каталог со сформированными файлами.
    pub(crate) output_dir: PathBuf,
    /// Срок, после которого сформированные файлы можно удалить.
    pub(crate) ttl: Duration,
}

