use std::{env, error::Error, ffi::OsString, fmt};

use tracing_subscriber::EnvFilter;

const DEFAULT_FILTER: &str = "chat_server=info,tower_http=info";

type BoxError = Box<dyn Error + Send + Sync + 'static>;

pub(crate) fn init() -> Result<(), TelemetryError> {
    let filter = filter_from(env::var_os("RUST_LOG"))?;
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .try_init()
        .map_err(TelemetryError::Initialize)
}

fn filter_from(value: Option<OsString>) -> Result<EnvFilter, TelemetryError> {
    let value = match value {
        Some(value) => value
            .into_string()
            .map_err(TelemetryError::FilterNotUnicode)?,
        None => String::from(DEFAULT_FILTER),
    };
    EnvFilter::try_new(value).map_err(|error| TelemetryError::InvalidFilter(Box::new(error)))
}

#[derive(Debug)]
pub(crate) enum TelemetryError {
    FilterNotUnicode(OsString),
    InvalidFilter(BoxError),
    Initialize(BoxError),
}

impl fmt::Display for TelemetryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FilterNotUnicode(value) => {
                write!(formatter, "RUST_LOG must be valid Unicode, got {value:?}")
            }
            Self::InvalidFilter(_) => formatter.write_str("RUST_LOG is not a valid tracing filter"),
            Self::Initialize(_) => formatter.write_str("failed to initialize tracing"),
        }
    }
}

impl Error for TelemetryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::FilterNotUnicode(_) => None,
            Self::InvalidFilter(error) | Self::Initialize(error) => Some(error.as_ref()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_filter_is_valid() {
        let filter = filter_from(None).expect("default filter is valid");
        let rendered = filter.to_string();

        assert!(rendered.contains("chat_server=info"));
        assert!(rendered.contains("tower_http=info"));
    }

    #[test]
    fn explicit_filter_is_parsed_strictly() {
        assert!(filter_from(Some(OsString::from("chat_server=debug"))).is_ok());
        assert!(matches!(
            filter_from(Some(OsString::from("chat_server=not-a-level"))),
            Err(TelemetryError::InvalidFilter(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn non_unicode_filter_is_rejected() {
        use std::os::unix::ffi::OsStringExt;

        let error =
            filter_from(Some(OsString::from_vec(vec![0xff]))).expect_err("filters must be Unicode");

        assert!(matches!(error, TelemetryError::FilterNotUnicode(_)));
    }
}
