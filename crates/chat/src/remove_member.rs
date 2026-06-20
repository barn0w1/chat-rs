use std::fmt;

use crate::{
    Chat, ChatEvent, ConversationId, Membership, MembershipRemoval, MembershipRole, UserId,
};

/// Requests removing a user from a conversation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RemoveMember {
    conversation_id: ConversationId,
    user_id: UserId,
}

impl RemoveMember {
    /// Creates a remove-member command.
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

    /// Returns the user to remove.
    pub const fn user_id(self) -> UserId {
        self.user_id
    }
}

/// The result of removing a member.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoveMemberResult {
    membership: Membership,
    events: Vec<ChatEvent>,
}

impl RemoveMemberResult {
    fn new(membership: Membership) -> Self {
        let events = vec![ChatEvent::MemberRemoved {
            membership: membership.clone(),
        }];
        Self { membership, events }
    }

    /// Returns the removed membership.
    pub const fn membership(&self) -> &Membership {
        &self.membership
    }

    /// Returns the events produced after persistence succeeded.
    pub fn events(&self) -> &[ChatEvent] {
        &self.events
    }
}

/// Describes why removing a member failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemoveMemberError {
    /// The target conversation does not exist.
    ConversationNotFound,
    /// The target user is not a member.
    NotMember,
    /// The actor may not remove the target member.
    NotAuthorized,
    /// The owner cannot leave or be removed.
    OwnerCannotLeave,
    /// The store returned inconsistent membership state.
    InvalidStoreResult,
    /// Persistence was temporarily unavailable.
    StoreUnavailable,
}

impl fmt::Display for RemoveMemberError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::ConversationNotFound => "conversation not found",
            Self::NotMember => "user is not a conversation member",
            Self::NotAuthorized => "the actor may not remove this member",
            Self::OwnerCannotLeave => "a conversation owner cannot be removed",
            Self::InvalidStoreResult => "the store returned invalid state",
            Self::StoreUnavailable => "the store is unavailable",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for RemoveMemberError {}

impl<S: crate::RemoveMemberStore> Chat<S> {
    /// Atomically authorizes and removes a conversation member.
    pub async fn remove_member(
        &self,
        actor_id: UserId,
        command: RemoveMember,
    ) -> Result<RemoveMemberResult, RemoveMemberError> {
        let membership = self
            .store()
            .remove_member(MembershipRemoval::new(
                actor_id,
                command.conversation_id,
                command.user_id,
            ))
            .await?;

        if membership.conversation_id() != command.conversation_id
            || membership.user_id() != command.user_id
            || membership.role() != MembershipRole::Member
        {
            return Err(RemoveMemberError::InvalidStoreResult);
        }

        Ok(RemoveMemberResult::new(membership))
    }
}
