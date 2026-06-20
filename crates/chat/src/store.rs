use std::{fmt, future::Future};

use crate::{Message, NewMessage};

/// Persistence operations required by the chat application.
///
/// New methods are added when a use case requires them. Implementations are
/// responsible for preserving the atomicity of state-dependent checks and
/// writes.
pub trait Store: Send + Sync {
    /// Persists a message and returns it with store-assigned metadata.
    ///
    /// Conversation existence, membership, ID assignment, and timestamp
    /// assignment must be handled atomically by the implementation.
    fn create_message(
        &self,
        message: NewMessage,
    ) -> impl Future<Output = Result<Message, StoreError>> + Send;
}

/// A semantic persistence failure understood by the application.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoreError {
    /// The target conversation does not exist.
    ConversationNotFound,
    /// The author is not a member of the target conversation.
    AuthorNotMember,
    /// The persistence service could not complete the operation.
    Unavailable,
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConversationNotFound => formatter.write_str("conversation not found"),
            Self::AuthorNotMember => formatter.write_str("author is not a conversation member"),
            Self::Unavailable => formatter.write_str("the store is unavailable"),
        }
    }
}

impl std::error::Error for StoreError {}
