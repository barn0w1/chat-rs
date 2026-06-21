//! Runtime integrations for the chat server.

#![warn(missing_docs)]

mod app;
mod auth;
mod config;
mod http;
mod server;

pub mod sqlite;

pub use config::{
    Config, ConfigError, DATABASE_PATH_ENV, LISTEN_ADDR_ENV, OIDC_CLIENT_ID_ENV,
    OIDC_CLIENT_SECRET_ENV, OIDC_ISSUER_ENV, OidcConfig, PUBLIC_URL_ENV,
};
pub use server::{OidcStartupError, RunError, run};
