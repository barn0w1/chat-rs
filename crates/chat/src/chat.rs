use crate::{
    MessageBody, NewMessage, PostMessage, PostMessageError, PostMessageResult, Store, UserId,
};

/// The entry point for chat use cases.
#[derive(Debug)]
pub struct Chat<S> {
    store: S,
}

impl<S> Chat<S> {
    /// Creates a chat application backed by `store`.
    pub const fn new(store: S) -> Self {
        Self { store }
    }
}

impl<S: Store> Chat<S> {
    /// Validates and persists a message for an authenticated user.
    pub async fn post_message(
        &self,
        author_id: UserId,
        command: PostMessage,
    ) -> Result<PostMessageResult, PostMessageError> {
        let (conversation_id, body) = command.into_parts();
        let body = MessageBody::try_from(body)?;
        let message = NewMessage::new(conversation_id, author_id, body);
        let message = self.store.create_message(message).await?;

        Ok(PostMessageResult::new(message))
    }
}
