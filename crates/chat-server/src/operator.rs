use std::{fmt, path::Path, time::Duration};

use crate::{
    auth::{AuthError, AuthStore},
    sqlite::{OpenError, SqliteStore},
};

/// Maximum lifetime accepted for an operator-created admission code.
pub const MAX_ADMISSION_CODE_LIFETIME_HOURS: u64 = 8_760;

/// A newly created admission code and its absolute expiration timestamp.
pub struct CreatedAdmissionCode {
    token: String,
    expires_at_ms: i64,
}

impl CreatedAdmissionCode {
    /// Exposes the bearer token that must be distributed to prospective users.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Returns the expiration instant as milliseconds since the Unix epoch.
    pub const fn expires_at_ms(&self) -> i64 {
        self.expires_at_ms
    }
}

impl fmt::Debug for CreatedAdmissionCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CreatedAdmissionCode")
            .field("token", &"[REDACTED]")
            .field("expires_at_ms", &self.expires_at_ms)
            .finish()
    }
}

/// Creates a reusable admission code in the configured SQLite database.
pub async fn create_admission_code(
    database_path: impl AsRef<Path>,
    valid_for_hours: u64,
) -> Result<CreatedAdmissionCode, CreateAdmissionCodeError> {
    if !(1..=MAX_ADMISSION_CODE_LIFETIME_HOURS).contains(&valid_for_hours) {
        return Err(CreateAdmissionCodeError::InvalidLifetime);
    }
    let valid_for = Duration::from_secs(valid_for_hours * 60 * 60);

    let store = SqliteStore::open(database_path)
        .await
        .map_err(CreateAdmissionCodeError::Database)?;
    let result = AuthStore::new(store.clone())
        .create_admission_code(valid_for, std::time::SystemTime::now())
        .await;
    store.close().await;

    let issued = result.map_err(CreateAdmissionCodeError::from_auth)?;
    Ok(CreatedAdmissionCode {
        token: issued.token(),
        expires_at_ms: issued.expires_at_ms(),
    })
}

/// Describes why an operator admission code could not be created.
#[derive(Debug)]
pub enum CreateAdmissionCodeError {
    /// The lifetime is zero or exceeds the supported maximum.
    InvalidLifetime,
    /// SQLite could not be opened or migrated.
    Database(OpenError),
    /// Secure random generation failed.
    Entropy(getrandom::Error),
    /// SQLite failed while creating the code.
    Store(sqlx::Error),
    /// The current time or requested expiration cannot be represented.
    TimeUnavailable,
    /// An unexpected authentication-store invariant failed.
    InvalidStoredData,
}

impl CreateAdmissionCodeError {
    fn from_auth(error: AuthError) -> Self {
        match error {
            AuthError::Entropy(error) => Self::Entropy(error),
            AuthError::StoreUnavailable(error) => Self::Store(error),
            AuthError::TimeUnavailable => Self::TimeUnavailable,
            AuthError::InvalidIdentity
            | AuthError::InvalidToken(_)
            | AuthError::InvalidStoredData
            | AuthError::LoginTransactionRejected => Self::InvalidStoredData,
        }
    }
}

impl fmt::Display for CreateAdmissionCodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLifetime => write!(
                formatter,
                "admission code lifetime must be between 1 and \
                 {MAX_ADMISSION_CODE_LIFETIME_HOURS} hours"
            ),
            Self::Database(_) => formatter.write_str("failed to prepare SQLite"),
            Self::Entropy(_) => formatter.write_str("secure random generation failed"),
            Self::Store(_) => formatter.write_str("failed to store admission code"),
            Self::TimeUnavailable => formatter.write_str("admission code expiration is invalid"),
            Self::InvalidStoredData => {
                formatter.write_str("admission code storage invariant failed")
            }
        }
    }
}

impl std::error::Error for CreateAdmissionCodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::Entropy(error) => Some(error),
            Self::Store(error) => Some(error),
            Self::InvalidLifetime | Self::TimeUnavailable | Self::InvalidStoredData => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AuthStore, SecretToken};

    #[tokio::test]
    async fn operator_creates_a_redacted_resolvable_code() {
        let directory = tempfile::tempdir().expect("temporary directory can be created");
        let path = directory.path().join("chat.sqlite3");
        let created = create_admission_code(&path, 24)
            .await
            .expect("operator code can be created");

        assert!(SecretToken::parse(created.token()).is_ok());
        assert!(!format!("{created:?}").contains(created.token()));
        let store = SqliteStore::open(&path)
            .await
            .expect("database can be reopened");
        assert!(
            AuthStore::new(store.clone())
                .resolve_admission_code(created.token(), std::time::SystemTime::now())
                .await
                .expect("code lookup succeeds")
                .is_some()
        );
        store.close().await;
    }

    #[tokio::test]
    async fn public_operation_enforces_lifetime_bounds_before_opening_sqlite() {
        for hours in [0, MAX_ADMISSION_CODE_LIFETIME_HOURS + 1] {
            let error = create_admission_code("unused.sqlite3", hours)
                .await
                .expect_err("invalid lifetime must be rejected");
            assert!(matches!(error, CreateAdmissionCodeError::InvalidLifetime));
        }
    }
}
