use std::{
    env,
    ffi::OsString,
    fmt,
    net::{AddrParseError, SocketAddr},
    path::{Path, PathBuf},
};

/// Environment variable used to configure the HTTP listen address.
pub const LISTEN_ADDR_ENV: &str = "CHAT_LISTEN_ADDR";

/// Environment variable used to configure the SQLite database path.
pub const DATABASE_PATH_ENV: &str = "CHAT_DATABASE_PATH";

const DEFAULT_DATABASE_PATH: &str = "chat.sqlite3";

/// Validated process configuration for the chat server.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    listen_addr: SocketAddr,
    database_path: PathBuf,
}

impl Config {
    /// Reads and validates configuration from the process environment.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_values(env::var_os(LISTEN_ADDR_ENV), env::var_os(DATABASE_PATH_ENV))
    }

    /// Returns the HTTP listen address.
    pub const fn listen_addr(&self) -> SocketAddr {
        self.listen_addr
    }

    /// Returns the SQLite database path.
    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    pub(crate) fn from_values(
        listen_addr: Option<OsString>,
        database_path: Option<OsString>,
    ) -> Result<Self, ConfigError> {
        let listen_addr = match listen_addr {
            Some(value) => {
                let value = value
                    .into_string()
                    .map_err(ConfigError::ListenAddressNotUnicode)?;
                value
                    .parse()
                    .map_err(|source| ConfigError::InvalidListenAddress { value, source })?
            }
            None => SocketAddr::from(([127, 0, 0, 1], 3000)),
        };

        let database_path = database_path.unwrap_or_else(|| OsString::from(DEFAULT_DATABASE_PATH));
        if database_path.is_empty() {
            return Err(ConfigError::EmptyDatabasePath);
        }

        Ok(Self {
            listen_addr,
            database_path: PathBuf::from(database_path),
        })
    }
}

/// Describes why server configuration is invalid.
#[derive(Debug)]
pub enum ConfigError {
    /// The listen address cannot be represented as Unicode for socket parsing.
    ListenAddressNotUnicode(OsString),
    /// The listen address is not an IP socket address.
    InvalidListenAddress {
        /// Rejected configuration value.
        value: String,
        /// Address parser error.
        source: AddrParseError,
    },
    /// The configured database path is empty.
    EmptyDatabasePath,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ListenAddressNotUnicode(value) => write!(
                formatter,
                "{LISTEN_ADDR_ENV} must be valid Unicode, got {value:?}"
            ),
            Self::InvalidListenAddress { value, .. } => write!(
                formatter,
                "{LISTEN_ADDR_ENV} must be an IP socket address, got {value:?}"
            ),
            Self::EmptyDatabasePath => {
                write!(formatter, "{DATABASE_PATH_ENV} cannot be empty")
            }
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidListenAddress { source, .. } => Some(source),
            Self::ListenAddressNotUnicode(_) | Self::EmptyDatabasePath => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn defaults_are_local_and_use_a_file_database() {
        let config = Config::from_values(None, None).expect("defaults are valid");

        assert_eq!(config.listen_addr(), "127.0.0.1:3000".parse().unwrap());
        assert_eq!(config.database_path(), Path::new("chat.sqlite3"));
    }

    #[test]
    fn values_override_defaults() {
        let config = Config::from_values(
            Some(OsString::from("[::1]:4000")),
            Some(OsString::from("data/chat.db")),
        )
        .expect("overrides are valid");

        assert_eq!(config.listen_addr(), "[::1]:4000".parse().unwrap());
        assert_eq!(config.database_path(), Path::new("data/chat.db"));
    }

    #[test]
    fn invalid_listen_address_is_rejected() {
        let error = Config::from_values(Some(OsString::from("localhost:3000")), None)
            .expect_err("hostnames are outside the configuration contract");

        assert!(matches!(error, ConfigError::InvalidListenAddress { .. }));
    }

    #[test]
    fn empty_database_path_is_rejected() {
        let error =
            Config::from_values(None, Some(OsString::new())).expect_err("an empty path is invalid");

        assert!(matches!(error, ConfigError::EmptyDatabasePath));
    }

    #[cfg(unix)]
    #[test]
    fn non_unicode_listen_address_is_rejected() {
        use std::os::unix::ffi::OsStringExt;

        let value = OsString::from_vec(vec![0xff]);
        let error =
            Config::from_values(Some(value), None).expect_err("socket addresses must be Unicode");

        assert!(matches!(error, ConfigError::ListenAddressNotUnicode(_)));
    }

    #[test]
    fn database_path_preserves_os_strings() {
        let path = OsStr::new("data/chat.sqlite3").to_owned();
        let config = Config::from_values(None, Some(path.clone())).expect("path is valid");

        assert_eq!(config.database_path().as_os_str(), path);
    }
}
