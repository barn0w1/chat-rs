use std::time::SystemTime;

use crate::{ConversationId, UserId};

/// A user's authority within a conversation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MembershipRole {
    /// May manage membership and post messages.
    Owner,
    /// May read and post messages.
    Member,
}

/// A user's membership in a conversation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Membership {
    conversation_id: ConversationId,
    user_id: UserId,
    role: MembershipRole,
    joined_at: SystemTime,
}

impl Membership {
    /// Creates a membership from values assigned by a store implementation.
    pub const fn new(
        conversation_id: ConversationId,
        user_id: UserId,
        role: MembershipRole,
        joined_at: SystemTime,
    ) -> Self {
        Self {
            conversation_id,
            user_id,
            role,
            joined_at,
        }
    }

    /// Returns the conversation ID.
    pub const fn conversation_id(&self) -> ConversationId {
        self.conversation_id
    }

    /// Returns the member's user ID.
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    /// Returns the member's role.
    pub const fn role(&self) -> MembershipRole {
        self.role
    }

    /// Returns when the user joined.
    pub const fn joined_at(&self) -> &SystemTime {
        &self.joined_at
    }
}

/// Values required to atomically add a conversation member.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NewMembership {
    actor_id: UserId,
    conversation_id: ConversationId,
    user_id: UserId,
}

impl NewMembership {
    pub(crate) const fn new(
        actor_id: UserId,
        conversation_id: ConversationId,
        user_id: UserId,
    ) -> Self {
        Self {
            actor_id,
            conversation_id,
            user_id,
        }
    }

    /// Returns the user requesting the operation.
    pub const fn actor_id(self) -> UserId {
        self.actor_id
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

/// Values required to atomically remove a conversation member.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MembershipRemoval {
    actor_id: UserId,
    conversation_id: ConversationId,
    user_id: UserId,
}

impl MembershipRemoval {
    pub(crate) const fn new(
        actor_id: UserId,
        conversation_id: ConversationId,
        user_id: UserId,
    ) -> Self {
        Self {
            actor_id,
            conversation_id,
            user_id,
        }
    }

    /// Returns the user requesting the operation.
    pub const fn actor_id(self) -> UserId {
        self.actor_id
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
