use std::fmt;

use crate::{Chat, ChatEvent, ConversationId, Membership, MembershipRole, NewMembership, UserId};

/// Requests adding a user to a conversation as a member.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AddMember {
    conversation_id: ConversationId,
    user_id: UserId,
}

impl AddMember {
    /// Creates an add-member command.
    pub const fn new(conversation_id: ConversationId, user_id: UserId) -> Self {
        Self {
            conversation_id,
            user_id,
        }
    }

    /// Returns the target conversation.
    pub const fn conversation_id(self) -> ConversationId {
        self.conversation_id
    }

    /// Returns the user to add.
    pub const fn user_id(self) -> UserId {
        self.user_id
    }
}

/// The result of adding a member.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AddMemberResult {
    membership: Membership,
    events: Vec<ChatEvent>,
}

impl AddMemberResult {
    fn new(membership: Membership) -> Result<Self, AddMemberError> {
        if membership.role() != MembershipRole::Member {
            return Err(AddMemberError::InvalidStoreResult);
        }

        let events = vec![ChatEvent::MemberAdded {
            membership: membership.clone(),
        }];
        Ok(Self { membership, events })
    }

    /// Returns the created membership.
    pub const fn membership(&self) -> &Membership {
        &self.membership
    }

    /// Returns the events produced after persistence succeeded.
    pub fn events(&self) -> &[ChatEvent] {
        &self.events
    }
}

/// Describes why adding a member failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddMemberError {
    /// The target conversation does not exist.
    ConversationNotFound,
    /// The user to add does not exist.
    UserNotFound,
    /// The user is already a member.
    AlreadyMember,
    /// The actor is not a conversation owner.
    NotOwner,
    /// The store returned inconsistent membership state.
    InvalidStoreResult,
    /// Persistence was temporarily unavailable.
    StoreUnavailable,
}

impl fmt::Display for AddMemberError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::ConversationNotFound => "conversation not found",
            Self::UserNotFound => "user not found",
            Self::AlreadyMember => "user is already a conversation member",
            Self::NotOwner => "only a conversation owner can add members",
            Self::InvalidStoreResult => "the store returned invalid state",
            Self::StoreUnavailable => "the store is unavailable",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for AddMemberError {}

impl<S: crate::AddMemberStore> Chat<S> {
    /// Atomically authorizes and adds a conversation member.
    pub async fn add_member(
        &self,
        actor_id: UserId,
        command: AddMember,
    ) -> Result<AddMemberResult, AddMemberError> {
        let membership = self
            .store()
            .add_member(NewMembership::new(
                actor_id,
                command.conversation_id,
                command.user_id,
            ))
            .await?;

        if membership.conversation_id() != command.conversation_id
            || membership.user_id() != command.user_id
        {
            return Err(AddMemberError::InvalidStoreResult);
        }

        AddMemberResult::new(membership)
    }
}
