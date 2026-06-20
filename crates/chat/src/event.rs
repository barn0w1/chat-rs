use crate::{Conversation, Membership, Message, User};

/// A fact produced by a successful chat use case.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChatEvent {
    /// A user was created.
    UserCreated {
        /// The created user.
        user: User,
    },
    /// A conversation and its owner membership were created.
    ConversationCreated {
        /// The created conversation.
        conversation: Conversation,
        /// The creator's owner membership.
        creator_membership: Membership,
    },
    /// A member was added to a conversation.
    MemberAdded {
        /// The created membership.
        membership: Membership,
    },
    /// A member was removed from a conversation.
    MemberRemoved {
        /// The removed membership.
        membership: Membership,
    },
    /// A message was stored and is ready for delivery.
    MessagePosted {
        /// The stored message.
        message: Message,
    },
}
