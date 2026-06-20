use std::fmt;

use crate::{Chat, Conversation, ConversationId, MembershipRole, UserId};

/// A conversation together with the requesting user's role.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversationDetails {
    conversation: Conversation,
    role: MembershipRole,
}

impl ConversationDetails {
    /// Creates conversation details returned by a read store.
    pub const fn new(conversation: Conversation, role: MembershipRole) -> Self {
        Self { conversation, role }
    }

    /// Returns the conversation.
    pub const fn conversation(&self) -> &Conversation {
        &self.conversation
    }

    /// Returns the requesting user's role.
    pub const fn role(&self) -> MembershipRole {
        self.role
    }
}

/// Describes why reading a conversation failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GetConversationError {
    /// The conversation does not exist or is not visible to the actor.
    NotFound,
    /// The store returned a conversation inconsistent with the request.
    InvalidStoreResult,
    /// Persistence was temporarily unavailable.
    StoreUnavailable,
}

impl fmt::Display for GetConversationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => formatter.write_str("conversation not found"),
            Self::InvalidStoreResult => formatter.write_str("the store returned invalid state"),
            Self::StoreUnavailable => formatter.write_str("the store is unavailable"),
        }
    }
}

impl std::error::Error for GetConversationError {}

impl<S: crate::GetConversationStore> Chat<S> {
    /// Returns a conversation visible to the requesting user.
    pub async fn get_conversation(
        &self,
        actor_id: UserId,
        conversation_id: ConversationId,
    ) -> Result<ConversationDetails, GetConversationError> {
        let details = self
            .store()
            .get_conversation(actor_id, conversation_id)
            .await?;
        if details.conversation().id() != conversation_id {
            return Err(GetConversationError::InvalidStoreResult);
        }
        Ok(details)
    }
}
