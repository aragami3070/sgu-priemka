//! Ошибки импорта, LDAP и тд.

mod app;
mod config;
mod import;
mod kerberos;
mod ldap;
mod result;

pub(crate) use app::AppError;
pub(crate) use config::ConfigError;
pub(crate) use import::{ImportError, UnsupportedGroupNumber};
pub(crate) use kerberos::KerberosError;
pub(crate) use ldap::{LdapError, LdapOperation, LdapPhase};
pub(crate) use result::ResultError;
