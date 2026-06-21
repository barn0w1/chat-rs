//! Application logic for chat.
//!
//! This crate owns chat concepts and use cases. Runtime integrations such as
//! WebSocket handling, authentication, and database implementations belong to
//! the server crate.

#![warn(missing_docs)]

mod add_member;
mod chat;
mod conversation;
mod create_conversation;
mod create_user;
mod event;
mod get_conversation;
mod id;
mod list_conversations;
mod list_members;
mod list_messages;
mod membership;
mod message;
mod post_message;
mod remove_member;
mod store;
mod user;

pub use add_member::{AddMember, AddMemberError, AddMemberResult};
pub use chat::Chat;
pub use conversation::{
    Conversation, ConversationTitle, ConversationTitleError, CreatedConversation,
    MAX_CONVERSATION_TITLE_CHARS, NewConversation,
};
pub use create_conversation::{
    CreateConversation, CreateConversationError, CreateConversationResult,
};
pub use create_user::{CreateUser, CreateUserError, CreateUserResult};
pub use event::ChatEvent;
pub use get_conversation::{ConversationDetails, GetConversationError};
pub use id::{ConversationId, InvalidId, MessageId, UserId};
pub use list_conversations::{
    ConversationPage, ConversationSummary, DEFAULT_CONVERSATION_PAGE_SIZE, ListConversations,
    ListConversationsError, MAX_CONVERSATION_PAGE_SIZE,
};
pub use list_members::{
    ConversationMember, DEFAULT_MEMBER_PAGE_SIZE, InvalidConversationMember, ListMembers,
    ListMembersError, MAX_MEMBER_PAGE_SIZE, MemberPage,
};
pub use list_messages::{
    DEFAULT_MESSAGE_PAGE_SIZE, ListMessages, ListMessagesError, MAX_MESSAGE_PAGE_SIZE, MessagePage,
};
pub use membership::{Membership, MembershipRemoval, MembershipRole, NewMembership};
pub use message::{MAX_MESSAGE_BODY_CHARS, Message, MessageBody, MessageBodyError, NewMessage};
pub use post_message::{PostMessage, PostMessageError, PostMessageResult};
pub use remove_member::{RemoveMember, RemoveMemberError, RemoveMemberResult};
pub use store::{
    AddMemberStore, CreateConversationStore, CreateUserStore, GetConversationStore,
    ListConversationsStore, ListMembersStore, ListMessagesStore, PostMessageStore, ReadStore,
    RemoveMemberStore, WriteStore,
};
pub use user::{DisplayName, DisplayNameError, MAX_DISPLAY_NAME_CHARS, NewUser, User};
