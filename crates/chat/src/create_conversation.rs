use std::fmt;

use crate::{
    Chat, ChatEvent, Conversation, ConversationTitle, ConversationTitleError, CreatedConversation,
    Membership, MembershipRole, NewConversation, UserId,
};

/// Requests creation of a conversation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateConversation {
    title: String,
}

impl CreateConversation {
    /// Creates a conversation command.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
        }
    }

    /// Returns the unvalidated title.
    pub fn title(&self) -> &str {
        &self.title
    }
}

/// The result of creating a conversation and its owner membership.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateConversationResult {
    conversation: Conversation,
    creator_membership: Membership,
    events: Vec<ChatEvent>,
}

impl CreateConversationResult {
    fn new(created: CreatedConversation) -> Result<Self, CreateConversationError> {
        let (conversation, creator_membership) = created.into_parts();
        if creator_membership.role() != MembershipRole::Owner
            || creator_membership.conversation_id() != conversation.id()
        {
            return Err(CreateConversationError::InvalidStoreResult);
        }

        let events = vec![ChatEvent::ConversationCreated {
            conversation: conversation.clone(),
            creator_membership: creator_membership.clone(),
        }];
        Ok(Self {
            conversation,
            creator_membership,
            events,
        })
    }

    /// Returns the created conversation.
    pub const fn conversation(&self) -> &Conversation {
        &self.conversation
    }

    /// Returns the creator's owner membership.
    pub const fn creator_membership(&self) -> &Membership {
        &self.creator_membership
    }

    /// Returns the events produced after persistence succeeded.
    pub fn events(&self) -> &[ChatEvent] {
        &self.events
    }
}

/// Describes why conversation creation failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreateConversationError {
    /// The supplied title is invalid.
    InvalidTitle(ConversationTitleError),
    /// The creator does not exist.
    CreatorNotFound,
    /// The store returned inconsistent conversation state.
    InvalidStoreResult,
    /// Persistence was temporarily unavailable.
    StoreUnavailable,
}

impl fmt::Display for CreateConversationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTitle(error) => error.fmt(formatter),
            Self::CreatorNotFound => formatter.write_str("conversation creator not found"),
            Self::InvalidStoreResult => formatter.write_str("the store returned invalid state"),
            Self::StoreUnavailable => formatter.write_str("the store is unavailable"),
        }
    }
}

impl std::error::Error for CreateConversationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidTitle(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ConversationTitleError> for CreateConversationError {
    fn from(error: ConversationTitleError) -> Self {
        Self::InvalidTitle(error)
    }
}

impl<S: crate::CreateConversationStore> Chat<S> {
    /// Validates and atomically creates a conversation and owner membership.
    pub async fn create_conversation(
        &self,
        creator_id: UserId,
        command: CreateConversation,
    ) -> Result<CreateConversationResult, CreateConversationError> {
        let title = ConversationTitle::try_from(command.title)?;
        let created = self
            .store()
            .create_conversation(NewConversation::new(creator_id, title.clone()))
            .await?;

        if created.conversation().title() != &title
            || created.creator_membership().user_id() != creator_id
        {
            return Err(CreateConversationError::InvalidStoreResult);
        }

        CreateConversationResult::new(created)
    }
}
