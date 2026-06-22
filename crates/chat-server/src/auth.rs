mod admission;
mod cookie;
mod oidc;
mod session;
mod store;

use std::{fmt, time::SystemTime};

use chat::{DisplayName, User, UserId};

pub(crate) use admission::{AdmissionCodeId, AdmissionOutcome, IssuedAdmissionCode};
pub(crate) use cookie::{CookieError, CookiePolicy};
pub(crate) use oidc::{OidcError, OidcProvider};
pub(crate) use session::{SecretToken, TokenError};
pub(crate) use store::AuthStore;

const DEFAULT_DISPLAY_NAME: &str = "New user";
const MAX_AUTHORITY_BYTES: usize = 2048;
const MAX_SUBJECT_BYTES: usize = 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VerifiedIdentity {
    authority: String,
    subject: String,
    display_name: DisplayName,
}

impl VerifiedIdentity {
    pub(crate) fn new(
        authority: impl Into<String>,
        subject: impl Into<String>,
        profile_name: Option<&str>,
    ) -> Result<Self, AuthError> {
        let authority = authority.into();
        let subject = subject.into();
        if authority.is_empty()
            || authority.len() > MAX_AUTHORITY_BYTES
            || subject.is_empty()
            || subject.len() > MAX_SUBJECT_BYTES
        {
            return Err(AuthError::InvalidIdentity);
        }

        let display_name = match profile_name.and_then(|name| DisplayName::try_from(name).ok()) {
            Some(display_name) => display_name,
            None => DisplayName::try_from(DEFAULT_DISPLAY_NAME)
                .map_err(|_| AuthError::InvalidIdentity)?,
        };

        Ok(Self {
            authority,
            subject,
            display_name,
        })
    }

    pub(crate) fn authority(&self) -> &str {
        &self.authority
    }

    pub(crate) fn subject(&self) -> &str {
        &self.subject
    }

    pub(crate) const fn display_name(&self) -> &DisplayName {
        &self.display_name
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AuthenticatedSession {
    user: User,
    csrf_token: SecretToken,
}

impl AuthenticatedSession {
    pub(crate) const fn new(user: User, csrf_token: SecretToken) -> Self {
        Self { user, csrf_token }
    }

    pub(crate) const fn user(&self) -> &User {
        &self.user
    }

    pub(crate) const fn user_id(&self) -> UserId {
        self.user.id()
    }

    pub(crate) fn verifies_csrf(&self, value: &str) -> bool {
        SecretToken::parse(value).is_ok_and(|token| token.matches(&self.csrf_token))
    }

    pub(crate) fn csrf_token(&self) -> String {
        self.csrf_token.encode()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct IssuedSession {
    session_token: SecretToken,
}

impl IssuedSession {
    pub(crate) const fn new(session_token: SecretToken) -> Self {
        Self { session_token }
    }

    pub(crate) fn session_token(&self) -> String {
        self.session_token.encode()
    }
}

#[derive(Clone)]
pub(crate) struct OidcLoginTransaction {
    state: SecretToken,
    browser_binding: SecretToken,
    nonce: String,
    pkce_verifier: String,
    admission_code_id: Option<AdmissionCodeId>,
}

impl fmt::Debug for OidcLoginTransaction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OidcLoginTransaction")
            .field("state", &"[REDACTED]")
            .field("browser_binding", &"[REDACTED]")
            .field("nonce", &"[REDACTED]")
            .field("pkce_verifier", &"[REDACTED]")
            .field("admission_code_id", &self.admission_code_id)
            .finish()
    }
}

impl OidcLoginTransaction {
    pub(crate) const fn new(
        state: SecretToken,
        browser_binding: SecretToken,
        nonce: String,
        pkce_verifier: String,
    ) -> Self {
        Self {
            state,
            browser_binding,
            nonce,
            pkce_verifier,
            admission_code_id: None,
        }
    }

    pub(crate) fn with_admission_code(mut self, admission_code_id: AdmissionCodeId) -> Self {
        self.admission_code_id = Some(admission_code_id);
        self
    }

    pub(crate) const fn state(&self) -> &SecretToken {
        &self.state
    }

    pub(crate) const fn browser_binding(&self) -> &SecretToken {
        &self.browser_binding
    }

    pub(crate) fn nonce(&self) -> &str {
        &self.nonce
    }

    pub(crate) fn pkce_verifier(&self) -> &str {
        &self.pkce_verifier
    }

    pub(crate) const fn admission_code_id(&self) -> Option<AdmissionCodeId> {
        self.admission_code_id
    }
}

#[derive(Clone)]
pub(crate) struct ConsumedOidcLogin {
    nonce: String,
    pkce_verifier: String,
    admission_code_id: Option<AdmissionCodeId>,
}

impl fmt::Debug for ConsumedOidcLogin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConsumedOidcLogin")
            .field("nonce", &"[REDACTED]")
            .field("pkce_verifier", &"[REDACTED]")
            .field("admission_code_id", &self.admission_code_id)
            .finish()
    }
}

impl ConsumedOidcLogin {
    pub(crate) const fn new(
        nonce: String,
        pkce_verifier: String,
        admission_code_id: Option<AdmissionCodeId>,
    ) -> Self {
        Self {
            nonce,
            pkce_verifier,
            admission_code_id,
        }
    }

    pub(crate) fn nonce(&self) -> &str {
        &self.nonce
    }

    pub(crate) fn pkce_verifier(&self) -> &str {
        &self.pkce_verifier
    }

    pub(crate) const fn admission_code_id(&self) -> Option<AdmissionCodeId> {
        self.admission_code_id
    }
}

#[derive(Debug)]
pub(crate) enum AuthError {
    Entropy(getrandom::Error),
    InvalidIdentity,
    InvalidToken(TokenError),
    InvalidStoredData,
    LoginCapacityReached,
    LoginTransactionRejected,
    StoreUnavailable(sqlx::Error),
    TimeUnavailable,
}

impl fmt::Display for AuthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Entropy(_) => formatter.write_str("secure random generation failed"),
            Self::InvalidIdentity => formatter.write_str("verified identity is invalid"),
            Self::InvalidToken(_) => formatter.write_str("authentication token is invalid"),
            Self::InvalidStoredData => formatter.write_str("authentication data is invalid"),
            Self::LoginCapacityReached => {
                formatter.write_str("too many authentication attempts are pending")
            }
            Self::LoginTransactionRejected => {
                formatter.write_str("login transaction is missing, expired, or mismatched")
            }
            Self::StoreUnavailable(_) => formatter.write_str("authentication store is unavailable"),
            Self::TimeUnavailable => formatter.write_str("system time is unavailable"),
        }
    }
}

impl std::error::Error for AuthError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Entropy(error) => Some(error),
            Self::InvalidToken(error) => Some(error),
            Self::StoreUnavailable(error) => Some(error),
            _ => None,
        }
    }
}

impl From<getrandom::Error> for AuthError {
    fn from(error: getrandom::Error) -> Self {
        Self::Entropy(error)
    }
}

impl From<TokenError> for AuthError {
    fn from(error: TokenError) -> Self {
        Self::InvalidToken(error)
    }
}

impl From<sqlx::Error> for AuthError {
    fn from(error: sqlx::Error) -> Self {
        Self::StoreUnavailable(error)
    }
}

pub(crate) fn unix_time_millis(now: SystemTime) -> Result<i64, AuthError> {
    let duration = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|_| AuthError::TimeUnavailable)?;
    i64::try_from(duration.as_millis()).map_err(|_| AuthError::TimeUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verified_identity_uses_valid_profile_or_neutral_fallback() {
        let named =
            VerifiedIdentity::new("issuer", "subject", Some(" Alice ")).expect("identity is valid");
        assert_eq!(named.display_name().as_str(), "Alice");

        let fallback =
            VerifiedIdentity::new("issuer", "subject", Some("   ")).expect("identity is valid");
        assert_eq!(fallback.display_name().as_str(), DEFAULT_DISPLAY_NAME);
    }

    #[test]
    fn verified_identity_rejects_empty_or_oversized_keys() {
        assert!(matches!(
            VerifiedIdentity::new("", "subject", None),
            Err(AuthError::InvalidIdentity)
        ));
        assert!(matches!(
            VerifiedIdentity::new("issuer", "x".repeat(MAX_SUBJECT_BYTES + 1), None),
            Err(AuthError::InvalidIdentity)
        ));
    }

    #[test]
    fn oidc_transaction_debug_output_redacts_protocol_secrets() {
        let transaction = OidcLoginTransaction::new(
            SecretToken::generate().expect("state can be generated"),
            SecretToken::generate().expect("binding can be generated"),
            String::from("private-nonce"),
            String::from("private-pkce-verifier"),
        );
        let debug = format!("{transaction:?}");

        assert!(!debug.contains("private-nonce"));
        assert!(!debug.contains("private-pkce-verifier"));
    }
}
