use std::{fmt, num::NonZeroU64};

/// The error returned when a database identifier is not positive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidId {
    value: i64,
}

impl InvalidId {
    /// Returns the rejected raw value.
    pub const fn value(self) -> i64 {
        self.value
    }
}

impl fmt::Display for InvalidId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "an ID must be positive, got {}", self.value)
    }
}

impl std::error::Error for InvalidId {}

macro_rules! define_id {
    ($(#[$metadata:meta])* $name:ident) => {
        $(#[$metadata])*
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(NonZeroU64);

        impl $name {
            /// Creates an ID from its positive database representation.
            pub const fn new(value: i64) -> Result<Self, InvalidId> {
                if value <= 0 {
                    return Err(InvalidId { value });
                }

                match NonZeroU64::new(value as u64) {
                    Some(value) => Ok(Self(value)),
                    None => Err(InvalidId { value: 0 }),
                }
            }

            /// Returns the database representation of this ID.
            pub const fn get(self) -> i64 {
                self.0.get() as i64
            }
        }

        impl TryFrom<i64> for $name {
            type Error = InvalidId;

            fn try_from(value: i64) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl From<$name> for i64 {
            fn from(value: $name) -> Self {
                value.get()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.get().fmt(formatter)
            }
        }
    };
}

define_id!(
    /// Identifies a user within one chat installation.
    UserId
);
define_id!(
    /// Identifies a conversation within one chat installation.
    ConversationId
);
define_id!(
    /// Identifies a message within one chat installation.
    MessageId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_accept_positive_database_values() {
        let id = UserId::new(42).expect("42 is a positive ID");

        assert_eq!(id.get(), 42);
        assert_eq!(id.to_string(), "42");
    }

    #[test]
    fn ids_reject_zero_and_negative_values() {
        assert_eq!(UserId::new(0), Err(InvalidId { value: 0 }));
        assert_eq!(ConversationId::new(-1), Err(InvalidId { value: -1 }));
        assert_eq!(MessageId::new(i64::MIN), Err(InvalidId { value: i64::MIN }));
    }
}
