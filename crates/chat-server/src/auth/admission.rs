use chat::User;

use super::AuthError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AdmissionCodeId(i64);

impl AdmissionCodeId {
    pub(crate) fn new(value: i64) -> Result<Self, AuthError> {
        if value > 0 {
            Ok(Self(value))
        } else {
            Err(AuthError::InvalidStoredData)
        }
    }

    pub(crate) const fn get(self) -> i64 {
        self.0
    }
}

#[derive(Clone, Debug)]
pub(crate) enum AdmissionOutcome {
    Admitted(User),
    Denied,
}

#[derive(Clone, Debug)]
pub(crate) struct IssuedAdmissionCode {
    token: super::SecretToken,
    expires_at_ms: i64,
}

impl IssuedAdmissionCode {
    pub(crate) const fn new(token: super::SecretToken, expires_at_ms: i64) -> Self {
        Self {
            token,
            expires_at_ms,
        }
    }

    pub(crate) fn token(&self) -> String {
        self.token.encode()
    }

    pub(crate) const fn expires_at_ms(&self) -> i64 {
        self.expires_at_ms
    }
}
