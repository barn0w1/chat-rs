use std::fmt;

use crate::{ChatEvent, ConversationId, Message, MessageBodyError, StoreError};

/// Requests a new message in a conversation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PostMessage {
    conversation_id: ConversationId,
    body: String,
}

impl PostMessage {
    /// Creates a post-message command.
    pub fn new(conversation_id: ConversationId, body: impl Into<String>) -> Self {
        Self {
            conversation_id,
            body: body.into(),
        }
    }

    /// Returns the target conversation.
    pub const fn conversation_id(&self) -> ConversationId {
        self.conversation_id
    }

    /// Returns the unvalidated message body.
    pub fn body(&self) -> &str {
        &self.body
    }

    pub(crate) fn into_parts(self) -> (ConversationId, String) {
        (self.conversation_id, self.body)
    }
}

/// The result of posting a message.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PostMessageResult {
    message: Message,
    events: Vec<ChatEvent>,
}

impl PostMessageResult {
    pub(crate) fn new(message: Message) -> Self {
        let events = vec![ChatEvent::MessagePosted {
            message: message.clone(),
        }];
        Self { message, events }
    }

    /// Returns the stored message.
    pub const fn message(&self) -> &Message {
        &self.message
    }

    /// Returns the events produced after persistence succeeded.
    pub fn events(&self) -> &[ChatEvent] {
        &self.events
    }

    /// Consumes the result and returns its parts.
    pub fn into_parts(self) -> (Message, Vec<ChatEvent>) {
        (self.message, self.events)
    }
}

/// Describes why a post-message request failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PostMessageError {
    /// The supplied body is invalid.
    InvalidBody(MessageBodyError),
    /// The target conversation does not exist.
    ConversationNotFound,
    /// The author is not a member of the target conversation.
    AuthorNotMember,
    /// Persistence was temporarily unavailable.
    StoreUnavailable,
}

impl fmt::Display for PostMessageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBody(error) => error.fmt(formatter),
            Self::ConversationNotFound => formatter.write_str("conversation not found"),
            Self::AuthorNotMember => formatter.write_str("author is not a conversation member"),
            Self::StoreUnavailable => formatter.write_str("the store is unavailable"),
        }
    }
}

impl std::error::Error for PostMessageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidBody(error) => Some(error),
            _ => None,
        }
    }
}

impl From<MessageBodyError> for PostMessageError {
    fn from(error: MessageBodyError) -> Self {
        Self::InvalidBody(error)
    }
}

impl From<StoreError> for PostMessageError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::ConversationNotFound => Self::ConversationNotFound,
            StoreError::AuthorNotMember => Self::AuthorNotMember,
            StoreError::Unavailable => Self::StoreUnavailable,
        }
    }
}
