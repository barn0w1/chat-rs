use std::fmt;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

const TOKEN_BYTES: usize = 32;

#[derive(Clone)]
pub(crate) struct SecretToken([u8; TOKEN_BYTES]);

impl SecretToken {
    pub(crate) fn generate() -> Result<Self, getrandom::Error> {
        let mut bytes = [0_u8; TOKEN_BYTES];
        getrandom::fill(&mut bytes)?;
        Ok(Self(bytes))
    }

    pub(crate) fn parse(value: &str) -> Result<Self, TokenError> {
        let bytes = URL_SAFE_NO_PAD.decode(value).map_err(|_| TokenError)?;
        bytes.try_into().map(Self).map_err(|_| TokenError)
    }

    pub(crate) fn encode(&self) -> String {
        URL_SAFE_NO_PAD.encode(self.0)
    }

    pub(crate) fn hash(&self) -> [u8; 32] {
        Sha256::digest(self.0).into()
    }

    pub(crate) const fn bytes(&self) -> &[u8; TOKEN_BYTES] {
        &self.0
    }

    pub(crate) fn from_stored(bytes: Vec<u8>) -> Result<Self, TokenError> {
        bytes.try_into().map(Self).map_err(|_| TokenError)
    }

    pub(crate) fn matches(&self, expected: &Self) -> bool {
        bool::from(self.0.ct_eq(&expected.0))
    }

    pub(crate) fn matches_hash(&self, expected: &[u8; 32]) -> bool {
        bool::from(self.hash().ct_eq(expected))
    }
}

impl fmt::Debug for SecretToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretToken([REDACTED])")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TokenError;

impl fmt::Display for TokenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("token must be 32 bytes of unpadded URL-safe Base64")
    }
}

impl std::error::Error for TokenError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_round_trip_and_require_exact_length() {
        let token = SecretToken([7; TOKEN_BYTES]);
        let encoded = token.encode();
        let parsed = SecretToken::parse(&encoded).expect("encoded token is valid");

        assert!(parsed.matches_hash(&token.hash()));
        assert!(SecretToken::parse("too-short").is_err());
    }
}
