//! Runtime integrations for the chat server.

#![warn(missing_docs)]

mod app;
mod config;
mod server;

pub mod sqlite;

pub use config::{Config, ConfigError, DATABASE_PATH_ENV, LISTEN_ADDR_ENV};
pub use server::{RunError, run};
