use std::{fmt, time::SystemTime};

use crate::{ConversationId, Membership, UserId};

/// Maximum number of Unicode scalar values accepted in a conversation title.
pub const MAX_CONVERSATION_TITLE_CHARS: usize = 100;

/// A validated conversation title.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversationTitle(String);

impl ConversationTitle {
    /// Returns the title as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the title and returns its string.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl TryFrom<String> for ConversationTitle {
    type Error = ConversationTitleError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let value = value.trim().to_owned();
        if value.is_empty() {
            return Err(ConversationTitleError::Empty);
        }
        if value.chars().any(char::is_control) {
            return Err(ConversationTitleError::ContainsControlCharacter);
        }

        let actual_chars = value.chars().count();
        if actual_chars > MAX_CONVERSATION_TITLE_CHARS {
            return Err(ConversationTitleError::TooLong {
                max_chars: MAX_CONVERSATION_TITLE_CHARS,
                actual_chars,
            });
        }

        Ok(Self(value))
    }
}

impl TryFrom<&str> for ConversationTitle {
    type Error = ConversationTitleError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_from(value.to_owned())
    }
}

impl AsRef<str> for ConversationTitle {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ConversationTitle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Describes why a conversation title is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConversationTitleError {
    /// The title contains no non-whitespace characters.
    Empty,
    /// The title contains a control character.
    ContainsControlCharacter,
    /// The title exceeds the configured character limit.
    TooLong {
        /// Maximum accepted character count.
        max_chars: usize,
        /// Character count of the rejected title.
        actual_chars: usize,
    },
}

impl fmt::Display for ConversationTitleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("a conversation title cannot be empty"),
            Self::ContainsControlCharacter => {
                formatter.write_str("a conversation title cannot contain control characters")
            }
            Self::TooLong {
                max_chars,
                actual_chars,
            } => write!(
                formatter,
                "a conversation title can contain at most {max_chars} characters, \
                 got {actual_chars}"
            ),
        }
    }
}

impl std::error::Error for ConversationTitleError {}

/// A conversation containing users and messages.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Conversation {
    id: ConversationId,
    title: ConversationTitle,
    created_at: SystemTime,
}

impl Conversation {
    /// Creates a conversation from values assigned by a store implementation.
    pub const fn new(id: ConversationId, title: ConversationTitle, created_at: SystemTime) -> Self {
        Self {
            id,
            title,
            created_at,
        }
    }

    /// Returns the conversation ID.
    pub const fn id(&self) -> ConversationId {
        self.id
    }

    /// Returns the title.
    pub const fn title(&self) -> &ConversationTitle {
        &self.title
    }

    /// Returns when the conversation was created.
    pub const fn created_at(&self) -> &SystemTime {
        &self.created_at
    }
}

/// A validated conversation awaiting store-assigned metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewConversation {
    creator_id: UserId,
    title: ConversationTitle,
}

impl NewConversation {
    pub(crate) const fn new(creator_id: UserId, title: ConversationTitle) -> Self {
        Self { creator_id, title }
    }

    /// Returns the creator, who must become the owner.
    pub const fn creator_id(&self) -> UserId {
        self.creator_id
    }

    /// Returns the requested title.
    pub const fn title(&self) -> &ConversationTitle {
        &self.title
    }
}

/// Values atomically created for a new conversation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreatedConversation {
    conversation: Conversation,
    creator_membership: Membership,
}

impl CreatedConversation {
    /// Creates the atomic store result.
    pub const fn new(conversation: Conversation, creator_membership: Membership) -> Self {
        Self {
            conversation,
            creator_membership,
        }
    }

    /// Returns the created conversation.
    pub const fn conversation(&self) -> &Conversation {
        &self.conversation
    }

    /// Returns the creator's owner membership.
    pub const fn creator_membership(&self) -> &Membership {
        &self.creator_membership
    }

    /// Consumes the result and returns its parts.
    pub fn into_parts(self) -> (Conversation, Membership) {
        (self.conversation, self.creator_membership)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversation_title_rejects_empty_and_long_input() {
        assert_eq!(
            ConversationTitle::try_from("\n\t"),
            Err(ConversationTitleError::Empty)
        );
        assert_eq!(
            ConversationTitle::try_from("General\nChat"),
            Err(ConversationTitleError::ContainsControlCharacter)
        );
        assert_eq!(
            ConversationTitle::try_from("a".repeat(MAX_CONVERSATION_TITLE_CHARS + 1)),
            Err(ConversationTitleError::TooLong {
                max_chars: MAX_CONVERSATION_TITLE_CHARS,
                actual_chars: MAX_CONVERSATION_TITLE_CHARS + 1,
            })
        );
    }
}
