//! Runtime integrations for the chat server.

#![warn(missing_docs)]

mod app;
mod auth;
mod config;
mod http;
mod operator;
mod server;

pub mod sqlite;

pub use config::{
    ADMISSION_MODE_ENV, AdmissionMode, Config, ConfigError, DATABASE_PATH_ENV, LISTEN_ADDR_ENV,
    OIDC_CLIENT_ID_ENV, OIDC_CLIENT_SECRET_ENV, OIDC_ISSUER_ENV, OidcConfig, PUBLIC_URL_ENV,
};
pub use operator::{
    CreateAdmissionCodeError, CreatedAdmissionCode, MAX_ADMISSION_CODE_LIFETIME_HOURS,
    create_admission_code,
};
pub use server::{OidcStartupError, RunError, run};
