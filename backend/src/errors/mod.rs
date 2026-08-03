//! Ошибки импорта, LDAP и тд.

mod app;
mod auth;
mod config;
mod import;
mod ldap;
mod result;

pub(crate) use app::AppError;
pub(crate) use auth::LdapAuthError;
pub(crate) use config::ConfigError;
pub(crate) use import::ImportError;
pub(crate) use ldap::LdapError;
pub(crate) use result::ResultError;
