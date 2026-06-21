use std::fmt;

use crate::{Chat, ConversationId, Message, MessageId, UserId};

/// Describes why reading one message failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GetMessageError {
    /// The message does not exist or is not visible to the actor.
    NotFound,
    /// The store returned a message inconsistent with the request.
    InvalidStoreResult,
    /// Persistence was temporarily unavailable.
    StoreUnavailable,
}

impl fmt::Display for GetMessageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => formatter.write_str("message not found"),
            Self::InvalidStoreResult => formatter.write_str("the store returned invalid state"),
            Self::StoreUnavailable => formatter.write_str("the store is unavailable"),
        }
    }
}

impl std::error::Error for GetMessageError {}

impl<S: crate::GetMessageStore> Chat<S> {
    /// Returns a message visible to the requesting user.
    pub async fn get_message(
        &self,
        actor_id: UserId,
        conversation_id: ConversationId,
        message_id: MessageId,
    ) -> Result<Message, GetMessageError> {
        let message = self
            .store()
            .get_message(actor_id, conversation_id, message_id)
            .await?;
        if message.id() != message_id || message.conversation_id() != conversation_id {
            return Err(GetMessageError::InvalidStoreResult);
        }
        Ok(message)
    }
}
