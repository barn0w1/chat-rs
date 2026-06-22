use std::{
    env,
    ffi::OsString,
    fmt,
    net::{AddrParseError, IpAddr, SocketAddr},
    path::{Path, PathBuf},
};

use url::{Host, ParseError, Url};

/// Environment variable used to configure the HTTP listen address.
pub const LISTEN_ADDR_ENV: &str = "CHAT_LISTEN_ADDR";

/// Environment variable used to configure the SQLite database path.
pub const DATABASE_PATH_ENV: &str = "CHAT_DATABASE_PATH";

/// Environment variable used to configure the externally visible server URL.
pub const PUBLIC_URL_ENV: &str = "CHAT_PUBLIC_URL";

/// Environment variable used to configure the OpenID Connect issuer.
pub const OIDC_ISSUER_ENV: &str = "CHAT_OIDC_ISSUER";

/// Environment variable used to configure the OpenID Connect client ID.
pub const OIDC_CLIENT_ID_ENV: &str = "CHAT_OIDC_CLIENT_ID";

/// Environment variable used to configure the optional OpenID Connect client secret.
pub const OIDC_CLIENT_SECRET_ENV: &str = "CHAT_OIDC_CLIENT_SECRET";

/// Environment variable used to configure new-user admission.
pub const ADMISSION_MODE_ENV: &str = "CHAT_ADMISSION_MODE";

const DEFAULT_DATABASE_PATH: &str = "chat.sqlite3";
const DEFAULT_PUBLIC_URL: &str = "http://127.0.0.1:3000";

/// Validated process configuration for the chat server.
#[derive(Clone, Eq, PartialEq)]
pub struct Config {
    listen_addr: SocketAddr,
    database_path: PathBuf,
    public_url: Url,
    oidc: Option<OidcConfig>,
    admission_mode: AdmissionMode,
}

impl Config {
    /// Reads and validates configuration from the process environment.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_values(ConfigValues {
            listen_addr: env::var_os(LISTEN_ADDR_ENV),
            database_path: env::var_os(DATABASE_PATH_ENV),
            public_url: env::var_os(PUBLIC_URL_ENV),
            oidc_issuer: env::var_os(OIDC_ISSUER_ENV),
            oidc_client_id: env::var_os(OIDC_CLIENT_ID_ENV),
            oidc_client_secret: env::var_os(OIDC_CLIENT_SECRET_ENV),
            admission_mode: env::var_os(ADMISSION_MODE_ENV),
        })
    }

    /// Returns the HTTP listen address.
    pub const fn listen_addr(&self) -> SocketAddr {
        self.listen_addr
    }

    /// Returns the SQLite database path.
    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    /// Returns the externally visible server URL.
    pub const fn public_url(&self) -> &Url {
        &self.public_url
    }

    /// Returns OpenID Connect configuration when login is enabled.
    pub const fn oidc(&self) -> Option<&OidcConfig> {
        self.oidc.as_ref()
    }

    /// Returns the policy applied when a verified identity has no local binding.
    pub const fn admission_mode(&self) -> AdmissionMode {
        self.admission_mode
    }

    pub(crate) fn from_values(values: ConfigValues) -> Result<Self, ConfigError> {
        let listen_addr = parse_listen_addr(values.listen_addr)?;

        let database_path = values
            .database_path
            .unwrap_or_else(|| OsString::from(DEFAULT_DATABASE_PATH));
        if database_path.is_empty() {
            return Err(ConfigError::EmptyDatabasePath);
        }

        let public_url = parse_public_url(values.public_url)?;
        let oidc = parse_oidc(
            values.oidc_issuer,
            values.oidc_client_id,
            values.oidc_client_secret,
        )?;
        let admission_mode = parse_admission_mode(values.admission_mode)?;

        Ok(Self {
            listen_addr,
            database_path: PathBuf::from(database_path),
            public_url,
            oidc,
            admission_mode,
        })
    }
}

impl fmt::Debug for Config {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Config")
            .field("listen_addr", &self.listen_addr)
            .field("database_path", &self.database_path)
            .field("public_url", &self.public_url)
            .field("oidc", &self.oidc)
            .field("admission_mode", &self.admission_mode)
            .finish()
    }
}

/// Validated OpenID Connect client configuration.
#[derive(Clone, Eq, PartialEq)]
pub struct OidcConfig {
    issuer: String,
    client_id: String,
    client_secret: Option<String>,
}

/// Policy for admitting a verified identity that has no local user binding.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AdmissionMode {
    /// Create a local user after successful identity verification.
    Open,
    /// Require an unexpired, operator-created admission code.
    #[default]
    InviteOnly,
}

impl OidcConfig {
    /// Returns the exact configured issuer identifier.
    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    /// Returns the configured client ID.
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    /// Returns the optional confidential-client secret.
    pub fn client_secret(&self) -> Option<&str> {
        self.client_secret.as_deref()
    }
}

impl fmt::Debug for OidcConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OidcConfig")
            .field("issuer", &self.issuer)
            .field("client_id", &self.client_id)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

#[derive(Default)]
pub(crate) struct ConfigValues {
    pub(crate) listen_addr: Option<OsString>,
    pub(crate) database_path: Option<OsString>,
    pub(crate) public_url: Option<OsString>,
    pub(crate) oidc_issuer: Option<OsString>,
    pub(crate) oidc_client_id: Option<OsString>,
    pub(crate) oidc_client_secret: Option<OsString>,
    pub(crate) admission_mode: Option<OsString>,
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
    /// A URL-valued environment variable is not Unicode.
    UrlNotUnicode {
        /// Name of the environment variable.
        variable: &'static str,
        /// Rejected operating-system string.
        value: OsString,
    },
    /// The public URL is syntactically invalid.
    InvalidPublicUrl {
        /// Rejected value.
        value: String,
        /// URL parser error.
        source: ParseError,
    },
    /// The public URL violates the server's origin contract.
    UnsupportedPublicUrl(&'static str),
    /// Exactly one of issuer or client ID was configured.
    IncompleteOidcConfiguration,
    /// An OpenID Connect string setting is empty.
    EmptyOidcValue {
        /// Name of the empty environment variable.
        variable: &'static str,
    },
    /// The OpenID Connect issuer URL is invalid.
    InvalidOidcIssuer {
        /// Rejected value.
        value: String,
        /// URL parser error.
        source: ParseError,
    },
    /// The OpenID Connect issuer URL is not HTTPS or loopback HTTP.
    UnsupportedOidcIssuer,
    /// The admission mode cannot be represented as Unicode.
    AdmissionModeNotUnicode(OsString),
    /// The admission mode is not one of the supported values.
    InvalidAdmissionMode(String),
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
            Self::UrlNotUnicode { variable, value } => {
                write!(formatter, "{variable} must be valid Unicode, got {value:?}")
            }
            Self::InvalidPublicUrl { value, .. } => {
                write!(formatter, "{PUBLIC_URL_ENV} is not a valid URL: {value:?}")
            }
            Self::UnsupportedPublicUrl(reason) => {
                write!(formatter, "{PUBLIC_URL_ENV} {reason}")
            }
            Self::IncompleteOidcConfiguration => write!(
                formatter,
                "{OIDC_ISSUER_ENV} and {OIDC_CLIENT_ID_ENV} must be set together"
            ),
            Self::EmptyOidcValue { variable } => write!(formatter, "{variable} cannot be empty"),
            Self::InvalidOidcIssuer { value, .. } => {
                write!(formatter, "{OIDC_ISSUER_ENV} is not a valid URL: {value:?}")
            }
            Self::UnsupportedOidcIssuer => write!(
                formatter,
                "{OIDC_ISSUER_ENV} must be an absolute HTTPS URL (or loopback HTTP) without user information, query, or fragment"
            ),
            Self::AdmissionModeNotUnicode(value) => write!(
                formatter,
                "{ADMISSION_MODE_ENV} must be valid Unicode, got {value:?}"
            ),
            Self::InvalidAdmissionMode(value) => write!(
                formatter,
                "{ADMISSION_MODE_ENV} must be open or invite_only, got {value:?}"
            ),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidListenAddress { source, .. } => Some(source),
            Self::InvalidPublicUrl { source, .. } | Self::InvalidOidcIssuer { source, .. } => {
                Some(source)
            }
            _ => None,
        }
    }
}

fn parse_listen_addr(value: Option<OsString>) -> Result<SocketAddr, ConfigError> {
    match value {
        Some(value) => {
            let value = value
                .into_string()
                .map_err(ConfigError::ListenAddressNotUnicode)?;
            value
                .parse()
                .map_err(|source| ConfigError::InvalidListenAddress { value, source })
        }
        None => Ok(SocketAddr::from(([127, 0, 0, 1], 3000))),
    }
}

fn parse_public_url(value: Option<OsString>) -> Result<Url, ConfigError> {
    let value =
        unicode_setting(value, PUBLIC_URL_ENV)?.unwrap_or_else(|| String::from(DEFAULT_PUBLIC_URL));
    let url =
        Url::parse(&value).map_err(|source| ConfigError::InvalidPublicUrl { value, source })?;

    if url.username() != "" || url.password().is_some() {
        return Err(ConfigError::UnsupportedPublicUrl(
            "cannot contain user information",
        ));
    }
    if url.path() != "/" || url.query().is_some() || url.fragment().is_some() {
        return Err(ConfigError::UnsupportedPublicUrl(
            "must be an origin without a path, query, or fragment",
        ));
    }
    validate_web_origin(&url).map_err(ConfigError::UnsupportedPublicUrl)?;
    Ok(url)
}

fn parse_oidc(
    issuer: Option<OsString>,
    client_id: Option<OsString>,
    client_secret: Option<OsString>,
) -> Result<Option<OidcConfig>, ConfigError> {
    let issuer = unicode_setting(issuer, OIDC_ISSUER_ENV)?;
    let client_id = unicode_setting(client_id, OIDC_CLIENT_ID_ENV)?;
    let client_secret = unicode_setting(client_secret, OIDC_CLIENT_SECRET_ENV)?;

    let (issuer, client_id) = match (issuer, client_id) {
        (Some(issuer), Some(client_id)) => (issuer, client_id),
        (None, None) if client_secret.is_none() => return Ok(None),
        _ => return Err(ConfigError::IncompleteOidcConfiguration),
    };

    if issuer.is_empty() {
        return Err(ConfigError::EmptyOidcValue {
            variable: OIDC_ISSUER_ENV,
        });
    }
    if client_id.is_empty() {
        return Err(ConfigError::EmptyOidcValue {
            variable: OIDC_CLIENT_ID_ENV,
        });
    }
    if client_secret.as_ref().is_some_and(String::is_empty) {
        return Err(ConfigError::EmptyOidcValue {
            variable: OIDC_CLIENT_SECRET_ENV,
        });
    }

    // Discovery compares issuer identifiers exactly. Validate the URL without
    // replacing the operator-provided serialization with `Url`'s serialization.
    let issuer_url = Url::parse(&issuer).map_err(|source| ConfigError::InvalidOidcIssuer {
        value: issuer.clone(),
        source,
    })?;
    if issuer_url.username() != ""
        || issuer_url.password().is_some()
        || issuer_url.host().is_none()
        || issuer_url.query().is_some()
        || issuer_url.fragment().is_some()
        || !is_https_or_loopback_http(&issuer_url)
    {
        return Err(ConfigError::UnsupportedOidcIssuer);
    }

    Ok(Some(OidcConfig {
        issuer,
        client_id,
        client_secret,
    }))
}

fn parse_admission_mode(value: Option<OsString>) -> Result<AdmissionMode, ConfigError> {
    let Some(value) = value else {
        return Ok(AdmissionMode::default());
    };
    let value = value
        .into_string()
        .map_err(ConfigError::AdmissionModeNotUnicode)?;
    match value.as_str() {
        "open" => Ok(AdmissionMode::Open),
        "invite_only" => Ok(AdmissionMode::InviteOnly),
        _ => Err(ConfigError::InvalidAdmissionMode(value)),
    }
}

fn unicode_setting(
    value: Option<OsString>,
    variable: &'static str,
) -> Result<Option<String>, ConfigError> {
    value
        .map(|value| {
            value
                .into_string()
                .map_err(|value| ConfigError::UrlNotUnicode { variable, value })
        })
        .transpose()
}

fn validate_web_origin(url: &Url) -> Result<(), &'static str> {
    if !matches!(url.scheme(), "http" | "https") || url.host().is_none() {
        return Err("must be an absolute HTTP or HTTPS origin");
    }
    if url.scheme() == "http" && !is_loopback_host(url.host()) {
        return Err("must use HTTPS unless the host is loopback");
    }
    Ok(())
}

fn is_https_or_loopback_http(url: &Url) -> bool {
    url.scheme() == "https" || (url.scheme() == "http" && is_loopback_host(url.host()))
}

fn is_loopback_host(host: Option<Host<&str>>) -> bool {
    match host {
        Some(Host::Ipv4(address)) => IpAddr::V4(address).is_loopback(),
        Some(Host::Ipv6(address)) => IpAddr::V6(address).is_loopback(),
        Some(Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn defaults_are_local_and_disable_oidc() {
        let config = Config::from_values(ConfigValues::default()).expect("defaults are valid");

        assert_eq!(config.listen_addr(), "127.0.0.1:3000".parse().unwrap());
        assert_eq!(config.database_path(), Path::new("chat.sqlite3"));
        assert_eq!(config.public_url().as_str(), "http://127.0.0.1:3000/");
        assert!(config.oidc().is_none());
        assert_eq!(config.admission_mode(), AdmissionMode::InviteOnly);
    }

    #[test]
    fn values_override_defaults_and_configure_oidc() {
        let config = Config::from_values(ConfigValues {
            listen_addr: Some(OsString::from("[::1]:4000")),
            database_path: Some(OsString::from("data/chat.db")),
            public_url: Some(OsString::from("https://chat.example.com")),
            oidc_issuer: Some(OsString::from("https://accounts.example.com/tenant")),
            oidc_client_id: Some(OsString::from("chat-client")),
            oidc_client_secret: Some(OsString::from("secret-value")),
            admission_mode: Some(OsString::from("open")),
        })
        .expect("overrides are valid");

        assert_eq!(config.listen_addr(), "[::1]:4000".parse().unwrap());
        assert_eq!(config.database_path(), Path::new("data/chat.db"));
        assert_eq!(config.public_url().as_str(), "https://chat.example.com/");
        let oidc = config.oidc().expect("OIDC is configured");
        assert_eq!(oidc.issuer(), "https://accounts.example.com/tenant");
        assert_eq!(oidc.client_id(), "chat-client");
        assert_eq!(oidc.client_secret(), Some("secret-value"));
        assert_eq!(config.admission_mode(), AdmissionMode::Open);
        assert!(!format!("{config:?}").contains("secret-value"));
    }

    #[test]
    fn oidc_issuer_preserves_the_exact_validated_identifier() {
        for issuer in [
            "https://accounts.google.com",
            "https://accounts.google.com/",
            "https://accounts.example.com/tenant",
        ] {
            let config = Config::from_values(ConfigValues {
                oidc_issuer: Some(OsString::from(issuer)),
                oidc_client_id: Some(OsString::from("client")),
                ..ConfigValues::default()
            })
            .expect("issuer is valid");

            assert_eq!(config.oidc().expect("OIDC is configured").issuer(), issuer);
        }
    }

    #[test]
    fn public_url_must_be_a_secure_origin_or_loopback() {
        for value in [
            "http://chat.example.com",
            "https://chat.example.com/path",
            "https://user@chat.example.com",
            "ftp://chat.example.com",
        ] {
            let error = Config::from_values(ConfigValues {
                public_url: Some(OsString::from(value)),
                ..ConfigValues::default()
            })
            .expect_err("invalid public URL must be rejected");
            assert!(matches!(error, ConfigError::UnsupportedPublicUrl(_)));
        }

        for value in ["http://localhost:3000", "http://[::1]:3000"] {
            assert!(
                Config::from_values(ConfigValues {
                    public_url: Some(OsString::from(value)),
                    ..ConfigValues::default()
                })
                .is_ok()
            );
        }
    }

    #[test]
    fn partial_or_empty_oidc_configuration_is_rejected() {
        let partial = Config::from_values(ConfigValues {
            oidc_issuer: Some(OsString::from("https://accounts.example.com")),
            ..ConfigValues::default()
        })
        .expect_err("issuer requires a client ID");
        assert!(matches!(partial, ConfigError::IncompleteOidcConfiguration));

        let empty_secret = Config::from_values(ConfigValues {
            oidc_issuer: Some(OsString::from("https://accounts.example.com")),
            oidc_client_id: Some(OsString::from("client")),
            oidc_client_secret: Some(OsString::new()),
            ..ConfigValues::default()
        })
        .expect_err("an explicitly empty secret is invalid");
        assert!(matches!(
            empty_secret,
            ConfigError::EmptyOidcValue {
                variable: OIDC_CLIENT_SECRET_ENV
            }
        ));
    }

    #[test]
    fn invalid_listen_address_and_empty_database_path_are_rejected() {
        let address = Config::from_values(ConfigValues {
            listen_addr: Some(OsString::from("localhost:3000")),
            ..ConfigValues::default()
        })
        .expect_err("hostnames are outside the listen-address contract");
        assert!(matches!(address, ConfigError::InvalidListenAddress { .. }));

        let path = Config::from_values(ConfigValues {
            database_path: Some(OsString::new()),
            ..ConfigValues::default()
        })
        .expect_err("an empty path is invalid");
        assert!(matches!(path, ConfigError::EmptyDatabasePath));
    }

    #[cfg(unix)]
    #[test]
    fn non_unicode_url_settings_are_rejected() {
        use std::os::unix::ffi::OsStringExt;

        let error = Config::from_values(ConfigValues {
            public_url: Some(OsString::from_vec(vec![0xff])),
            ..ConfigValues::default()
        })
        .expect_err("URLs must be Unicode");
        assert!(matches!(error, ConfigError::UrlNotUnicode { .. }));

        let error = Config::from_values(ConfigValues {
            admission_mode: Some(OsString::from_vec(vec![0xff])),
            ..ConfigValues::default()
        })
        .expect_err("admission mode must be Unicode");
        assert!(matches!(error, ConfigError::AdmissionModeNotUnicode(_)));
    }

    #[test]
    fn database_path_preserves_os_strings() {
        let path = OsStr::new("data/chat.sqlite3").to_owned();
        let config = Config::from_values(ConfigValues {
            database_path: Some(path.clone()),
            ..ConfigValues::default()
        })
        .expect("path is valid");

        assert_eq!(config.database_path().as_os_str(), path);
    }

    #[test]
    fn admission_mode_is_strict() {
        for value in ["", "closed", "OPEN", "invite-only"] {
            let error = Config::from_values(ConfigValues {
                admission_mode: Some(OsString::from(value)),
                ..ConfigValues::default()
            })
            .expect_err("unsupported admission mode must be rejected");
            assert!(matches!(error, ConfigError::InvalidAdmissionMode(_)));
        }
    }
}
