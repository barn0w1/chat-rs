use std::fmt;

use crate::{Chat, ConversationId, Membership, User, UserId};

/// A user and their conversation membership.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversationMember {
    user: User,
    membership: Membership,
}

impl ConversationMember {
    /// Creates a member entry returned by a read store.
    pub fn new(user: User, membership: Membership) -> Result<Self, InvalidConversationMember> {
        if user.id() != membership.user_id() {
            return Err(InvalidConversationMember);
        }
        Ok(Self { user, membership })
    }

    /// Returns the user.
    pub const fn user(&self) -> &User {
        &self.user
    }

    /// Returns the membership.
    pub const fn membership(&self) -> &Membership {
        &self.membership
    }
}

/// The error returned for mismatched user and membership values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidConversationMember;

impl fmt::Display for InvalidConversationMember {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the user does not match the membership")
    }
}

impl std::error::Error for InvalidConversationMember {}

/// The result of listing conversation members.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListMembersResult {
    members: Vec<ConversationMember>,
}

impl ListMembersResult {
    fn new(members: Vec<ConversationMember>) -> Self {
        Self { members }
    }

    /// Returns the members.
    pub fn members(&self) -> &[ConversationMember] {
        &self.members
    }

    /// Consumes the result and returns the entries.
    pub fn into_members(self) -> Vec<ConversationMember> {
        self.members
    }
}

/// Describes why listing members failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ListMembersError {
    /// The conversation does not exist or is not visible to the actor.
    NotFound,
    /// The store returned membership inconsistent with the request.
    InvalidStoreResult,
    /// Persistence was temporarily unavailable.
    StoreUnavailable,
}

impl fmt::Display for ListMembersError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => formatter.write_str("conversation not found"),
            Self::InvalidStoreResult => formatter.write_str("the store returned invalid state"),
            Self::StoreUnavailable => formatter.write_str("the store is unavailable"),
        }
    }
}

impl std::error::Error for ListMembersError {}

impl<S: crate::ListMembersStore> Chat<S> {
    /// Returns members when the requesting user may view the conversation.
    pub async fn list_members(
        &self,
        actor_id: UserId,
        conversation_id: ConversationId,
    ) -> Result<ListMembersResult, ListMembersError> {
        let members = self.store().list_members(actor_id, conversation_id).await?;
        if members
            .iter()
            .any(|member| member.membership().conversation_id() != conversation_id)
        {
            return Err(ListMembersError::InvalidStoreResult);
        }
        Ok(ListMembersResult::new(members))
    }
}
