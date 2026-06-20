use std::{fmt, time::SystemTime};

use crate::{ConversationId, MessageId, UserId};

/// Maximum number of Unicode scalar values accepted in a message body.
pub const MAX_MESSAGE_BODY_CHARS: usize = 4_000;

/// A validated chat message body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessageBody(String);

impl MessageBody {
    /// Returns the body as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the body and returns its string.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl TryFrom<String> for MessageBody {
    type Error = MessageBodyError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.trim().is_empty() {
            return Err(MessageBodyError::Empty);
        }

        let actual_chars = value.chars().count();
        if actual_chars > MAX_MESSAGE_BODY_CHARS {
            return Err(MessageBodyError::TooLong {
                max_chars: MAX_MESSAGE_BODY_CHARS,
                actual_chars,
            });
        }

        Ok(Self(value))
    }
}

impl TryFrom<&str> for MessageBody {
    type Error = MessageBodyError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_from(value.to_owned())
    }
}

impl AsRef<str> for MessageBody {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for MessageBody {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Describes why a message body is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageBodyError {
    /// The body contains no non-whitespace characters.
    Empty,
    /// The body exceeds the configured character limit.
    TooLong {
        /// Maximum accepted character count.
        max_chars: usize,
        /// Character count of the rejected body.
        actual_chars: usize,
    },
}

impl fmt::Display for MessageBodyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("a message body cannot be empty"),
            Self::TooLong {
                max_chars,
                actual_chars,
            } => write!(
                formatter,
                "a message body can contain at most {max_chars} characters, got {actual_chars}"
            ),
        }
    }
}

impl std::error::Error for MessageBodyError {}

/// A message stored by the chat application.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Message {
    id: MessageId,
    conversation_id: ConversationId,
    author_id: UserId,
    body: MessageBody,
    created_at: SystemTime,
}

impl Message {
    /// Creates a message from values assigned by a store implementation.
    pub fn new(
        id: MessageId,
        conversation_id: ConversationId,
        author_id: UserId,
        body: MessageBody,
        created_at: SystemTime,
    ) -> Self {
        Self {
            id,
            conversation_id,
            author_id,
            body,
            created_at,
        }
    }

    /// Returns the message ID.
    pub const fn id(&self) -> MessageId {
        self.id
    }

    /// Returns the conversation containing the message.
    pub const fn conversation_id(&self) -> ConversationId {
        self.conversation_id
    }

    /// Returns the author of the message.
    pub const fn author_id(&self) -> UserId {
        self.author_id
    }

    /// Returns the validated message body.
    pub const fn body(&self) -> &MessageBody {
        &self.body
    }

    /// Returns when the message was created.
    pub const fn created_at(&self) -> &SystemTime {
        &self.created_at
    }
}

/// A validated message awaiting store-assigned metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewMessage {
    conversation_id: ConversationId,
    author_id: UserId,
    body: MessageBody,
}

impl NewMessage {
    pub(crate) const fn new(
        conversation_id: ConversationId,
        author_id: UserId,
        body: MessageBody,
    ) -> Self {
        Self {
            conversation_id,
            author_id,
            body,
        }
    }

    /// Returns the target conversation.
    pub const fn conversation_id(&self) -> ConversationId {
        self.conversation_id
    }

    /// Returns the author requesting the post.
    pub const fn author_id(&self) -> UserId {
        self.author_id
    }

    /// Returns the validated body.
    pub const fn body(&self) -> &MessageBody {
        &self.body
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_body_preserves_valid_input() {
        let body = MessageBody::try_from("  hello  ").expect("the body is valid");

        assert_eq!(body.as_str(), "  hello  ");
    }

    #[test]
    fn message_body_rejects_whitespace_only_input() {
        assert_eq!(MessageBody::try_from(" \n\t"), Err(MessageBodyError::Empty));
    }

    #[test]
    fn message_body_counts_unicode_scalars() {
        let accepted = "界".repeat(MAX_MESSAGE_BODY_CHARS);
        let rejected = "界".repeat(MAX_MESSAGE_BODY_CHARS + 1);

        assert!(MessageBody::try_from(accepted).is_ok());
        assert_eq!(
            MessageBody::try_from(rejected),
            Err(MessageBodyError::TooLong {
                max_chars: MAX_MESSAGE_BODY_CHARS,
                actual_chars: MAX_MESSAGE_BODY_CHARS + 1,
            })
        );
    }
}
