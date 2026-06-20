use std::future::Future;

use crate::{
    AddMemberError, ConversationDetails, ConversationId, ConversationMember, ConversationSummary,
    CreateConversationError, CreateUserError, CreatedConversation, GetConversationError,
    ListConversationsError, ListMembersError, ListMessages, ListMessagesError, Membership,
    MembershipRemoval, Message, MessagePage, NewConversation, NewMembership, NewMessage, NewUser,
    PostMessageError, RemoveMemberError, User, UserId,
};

/// Store capability required to create users.
pub trait CreateUserStore: Send + Sync {
    /// Persists a user and returns store-assigned metadata.
    fn create_user(
        &self,
        user: NewUser,
    ) -> impl Future<Output = Result<User, CreateUserError>> + Send;
}

/// Store capability required to create conversations.
pub trait CreateConversationStore: Send + Sync {
    /// Atomically creates a conversation and its owner membership.
    fn create_conversation(
        &self,
        conversation: NewConversation,
    ) -> impl Future<Output = Result<CreatedConversation, CreateConversationError>> + Send;
}

/// Store capability required to add conversation members.
pub trait AddMemberStore: Send + Sync {
    /// Atomically authorizes and creates a member membership.
    fn add_member(
        &self,
        membership: NewMembership,
    ) -> impl Future<Output = Result<Membership, AddMemberError>> + Send;
}

/// Store capability required to remove conversation members.
pub trait RemoveMemberStore: Send + Sync {
    /// Atomically authorizes and removes a membership.
    fn remove_member(
        &self,
        removal: MembershipRemoval,
    ) -> impl Future<Output = Result<Membership, RemoveMemberError>> + Send;
}

/// Store capability required to post messages.
pub trait PostMessageStore: Send + Sync {
    /// Atomically verifies membership and persists a message.
    fn create_message(
        &self,
        message: NewMessage,
    ) -> impl Future<Output = Result<Message, PostMessageError>> + Send;
}

/// Complete set of mutation capabilities currently required by the core.
pub trait WriteStore:
    CreateUserStore + CreateConversationStore + AddMemberStore + RemoveMemberStore + PostMessageStore
{
}

impl<T> WriteStore for T where
    T: CreateUserStore
        + CreateConversationStore
        + AddMemberStore
        + RemoveMemberStore
        + PostMessageStore
{
}

/// Store capability required to read one conversation.
pub trait GetConversationStore: Send + Sync {
    /// Returns a conversation visible to `actor_id`.
    fn get_conversation(
        &self,
        actor_id: UserId,
        conversation_id: ConversationId,
    ) -> impl Future<Output = Result<ConversationDetails, GetConversationError>> + Send;
}

/// Store capability required to list a user's conversations.
pub trait ListConversationsStore: Send + Sync {
    /// Returns conversations visible to `actor_id`.
    fn list_conversations(
        &self,
        actor_id: UserId,
    ) -> impl Future<Output = Result<Vec<ConversationSummary>, ListConversationsError>> + Send;
}

/// Store capability required to list conversation members.
pub trait ListMembersStore: Send + Sync {
    /// Returns members when `actor_id` may view the conversation.
    fn list_members(
        &self,
        actor_id: UserId,
        conversation_id: ConversationId,
    ) -> impl Future<Output = Result<Vec<ConversationMember>, ListMembersError>> + Send;
}

/// Store capability required to list messages.
pub trait ListMessagesStore: Send + Sync {
    /// Returns a page when `actor_id` may view the conversation.
    fn list_messages(
        &self,
        actor_id: UserId,
        query: ListMessages,
    ) -> impl Future<Output = Result<MessagePage, ListMessagesError>> + Send;
}

/// Complete set of query capabilities currently required by the core.
pub trait ReadStore:
    GetConversationStore + ListConversationsStore + ListMembersStore + ListMessagesStore
{
}

impl<T> ReadStore for T where
    T: GetConversationStore + ListConversationsStore + ListMembersStore + ListMessagesStore
{
}
