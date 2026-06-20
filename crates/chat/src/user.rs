use std::{fmt, time::SystemTime};

use crate::UserId;

/// Maximum number of Unicode scalar values accepted in a display name.
pub const MAX_DISPLAY_NAME_CHARS: usize = 64;

/// A validated user-facing name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisplayName(String);

impl DisplayName {
    /// Returns the display name as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the display name and returns its string.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl TryFrom<String> for DisplayName {
    type Error = DisplayNameError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let value = value.trim().to_owned();
        if value.is_empty() {
            return Err(DisplayNameError::Empty);
        }
        if value.chars().any(char::is_control) {
            return Err(DisplayNameError::ContainsControlCharacter);
        }

        let actual_chars = value.chars().count();
        if actual_chars > MAX_DISPLAY_NAME_CHARS {
            return Err(DisplayNameError::TooLong {
                max_chars: MAX_DISPLAY_NAME_CHARS,
                actual_chars,
            });
        }

        Ok(Self(value))
    }
}

impl TryFrom<&str> for DisplayName {
    type Error = DisplayNameError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_from(value.to_owned())
    }
}

impl AsRef<str> for DisplayName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for DisplayName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Describes why a display name is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayNameError {
    /// The name contains no non-whitespace characters.
    Empty,
    /// The name contains a control character.
    ContainsControlCharacter,
    /// The name exceeds the configured character limit.
    TooLong {
        /// Maximum accepted character count.
        max_chars: usize,
        /// Character count of the rejected name.
        actual_chars: usize,
    },
}

impl fmt::Display for DisplayNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("a display name cannot be empty"),
            Self::ContainsControlCharacter => {
                formatter.write_str("a display name cannot contain control characters")
            }
            Self::TooLong {
                max_chars,
                actual_chars,
            } => write!(
                formatter,
                "a display name can contain at most {max_chars} characters, got {actual_chars}"
            ),
        }
    }
}

impl std::error::Error for DisplayNameError {}

/// A user known to the chat application.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct User {
    id: UserId,
    display_name: DisplayName,
    created_at: SystemTime,
}

impl User {
    /// Creates a user from values assigned by a store implementation.
    pub const fn new(id: UserId, display_name: DisplayName, created_at: SystemTime) -> Self {
        Self {
            id,
            display_name,
            created_at,
        }
    }

    /// Returns the user ID.
    pub const fn id(&self) -> UserId {
        self.id
    }

    /// Returns the display name.
    pub const fn display_name(&self) -> &DisplayName {
        &self.display_name
    }

    /// Returns when the user was created.
    pub const fn created_at(&self) -> &SystemTime {
        &self.created_at
    }
}

/// A validated user awaiting store-assigned metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewUser {
    display_name: DisplayName,
}

impl NewUser {
    pub(crate) const fn new(display_name: DisplayName) -> Self {
        Self { display_name }
    }

    /// Returns the requested display name.
    pub const fn display_name(&self) -> &DisplayName {
        &self.display_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_name_preserves_valid_input() {
        let name = DisplayName::try_from(" Yuito ").expect("the name is valid");

        assert_eq!(name.as_str(), "Yuito");
    }

    #[test]
    fn display_name_rejects_empty_and_long_input() {
        assert_eq!(DisplayName::try_from("  "), Err(DisplayNameError::Empty));
        assert_eq!(
            DisplayName::try_from("Yui\nto"),
            Err(DisplayNameError::ContainsControlCharacter)
        );
        assert_eq!(
            DisplayName::try_from("a".repeat(MAX_DISPLAY_NAME_CHARS + 1)),
            Err(DisplayNameError::TooLong {
                max_chars: MAX_DISPLAY_NAME_CHARS,
                actual_chars: MAX_DISPLAY_NAME_CHARS + 1,
            })
        );
    }
}
