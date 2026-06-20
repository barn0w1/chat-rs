//! Application logic for chat.
//!
//! This crate owns chat concepts and use cases. Runtime integrations such as
//! WebSocket handling, authentication, and database implementations belong to
//! the server crate.

#![warn(missing_docs)]

mod chat;
mod event;
mod id;
mod message;
mod post_message;
mod store;

pub use chat::Chat;
pub use event::ChatEvent;
pub use id::{ConversationId, InvalidId, MessageId, UserId};
pub use message::{MAX_MESSAGE_BODY_CHARS, Message, MessageBody, MessageBodyError, NewMessage};
pub use post_message::{PostMessage, PostMessageError, PostMessageResult};
pub use store::{Store, StoreError};
