use std::{collections::HashSet, fmt};

use crate::{Chat, Conversation, MembershipRole, UserId};

/// A conversation entry visible to one user.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversationSummary {
    conversation: Conversation,
    role: MembershipRole,
}

impl ConversationSummary {
    /// Creates a summary returned by a read store.
    pub const fn new(conversation: Conversation, role: MembershipRole) -> Self {
        Self { conversation, role }
    }

    /// Returns the conversation.
    pub const fn conversation(&self) -> &Conversation {
        &self.conversation
    }

    /// Returns the user's role.
    pub const fn role(&self) -> MembershipRole {
        self.role
    }
}

/// The result of listing a user's conversations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListConversationsResult {
    conversations: Vec<ConversationSummary>,
}

impl ListConversationsResult {
    fn new(conversations: Vec<ConversationSummary>) -> Self {
        Self { conversations }
    }

    /// Returns visible conversations.
    pub fn conversations(&self) -> &[ConversationSummary] {
        &self.conversations
    }

    /// Consumes the result and returns the entries.
    pub fn into_conversations(self) -> Vec<ConversationSummary> {
        self.conversations
    }
}

/// Describes why listing conversations failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ListConversationsError {
    /// The store returned duplicate conversations.
    InvalidStoreResult,
    /// Persistence was temporarily unavailable.
    StoreUnavailable,
}

impl fmt::Display for ListConversationsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidStoreResult => formatter.write_str("the store returned invalid state"),
            Self::StoreUnavailable => formatter.write_str("the store is unavailable"),
        }
    }
}

impl std::error::Error for ListConversationsError {}

impl<S: crate::ListConversationsStore> Chat<S> {
    /// Returns conversations visible to the requesting user.
    pub async fn list_conversations(
        &self,
        actor_id: UserId,
    ) -> Result<ListConversationsResult, ListConversationsError> {
        let conversations = self.store().list_conversations(actor_id).await?;
        let mut ids = HashSet::with_capacity(conversations.len());
        if conversations
            .iter()
            .any(|entry| !ids.insert(entry.conversation().id()))
        {
            return Err(ListConversationsError::InvalidStoreResult);
        }
        Ok(ListConversationsResult::new(conversations))
    }
}
