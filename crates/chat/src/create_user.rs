use std::fmt;

use crate::{Chat, ChatEvent, DisplayName, DisplayNameError, NewUser, User};

/// Requests creation of a chat user.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateUser {
    display_name: String,
}

impl CreateUser {
    /// Creates a user command.
    pub fn new(display_name: impl Into<String>) -> Self {
        Self {
            display_name: display_name.into(),
        }
    }

    /// Returns the unvalidated display name.
    pub fn display_name(&self) -> &str {
        &self.display_name
    }
}

/// The result of creating a user.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateUserResult {
    user: User,
    events: Vec<ChatEvent>,
}

impl CreateUserResult {
    fn new(user: User) -> Self {
        let events = vec![ChatEvent::UserCreated { user: user.clone() }];
        Self { user, events }
    }

    /// Returns the created user.
    pub const fn user(&self) -> &User {
        &self.user
    }

    /// Returns the events produced after persistence succeeded.
    pub fn events(&self) -> &[ChatEvent] {
        &self.events
    }

    /// Consumes the result and returns its parts.
    pub fn into_parts(self) -> (User, Vec<ChatEvent>) {
        (self.user, self.events)
    }
}

/// Describes why user creation failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreateUserError {
    /// The supplied display name is invalid.
    InvalidDisplayName(DisplayNameError),
    /// The store returned a user inconsistent with the request.
    InvalidStoreResult,
    /// Persistence was temporarily unavailable.
    StoreUnavailable,
}

impl fmt::Display for CreateUserError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDisplayName(error) => error.fmt(formatter),
            Self::InvalidStoreResult => formatter.write_str("the store returned invalid state"),
            Self::StoreUnavailable => formatter.write_str("the store is unavailable"),
        }
    }
}

impl std::error::Error for CreateUserError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidDisplayName(error) => Some(error),
            _ => None,
        }
    }
}

impl<S: crate::CreateUserStore> Chat<S> {
    /// Validates and creates a user.
    pub async fn create_user(
        &self,
        command: CreateUser,
    ) -> Result<CreateUserResult, CreateUserError> {
        let display_name = DisplayName::try_from(command.display_name)?;
        let user = self
            .store()
            .create_user(NewUser::new(display_name.clone()))
            .await?;
        if user.display_name() != &display_name {
            return Err(CreateUserError::InvalidStoreResult);
        }

        Ok(CreateUserResult::new(user))
    }
}

impl From<DisplayNameError> for CreateUserError {
    fn from(error: DisplayNameError) -> Self {
        Self::InvalidDisplayName(error)
    }
}
