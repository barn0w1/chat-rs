use std::fmt;

use crate::{Chat, ConversationId, Message, MessageId, UserId};

/// Default number of messages requested in one page.
pub const DEFAULT_MESSAGE_PAGE_SIZE: usize = 50;

/// Maximum number of messages accepted in one page request.
pub const MAX_MESSAGE_PAGE_SIZE: usize = 100;

/// Requests a page of messages older than an optional cursor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ListMessages {
    conversation_id: ConversationId,
    before: Option<MessageId>,
    limit: usize,
}

impl ListMessages {
    /// Creates a query using the default page size.
    pub const fn new(conversation_id: ConversationId) -> Self {
        Self {
            conversation_id,
            before: None,
            limit: DEFAULT_MESSAGE_PAGE_SIZE,
        }
    }

    /// Sets the exclusive upper message-ID cursor.
    pub const fn before(mut self, message_id: MessageId) -> Self {
        self.before = Some(message_id);
        self
    }

    /// Sets the requested page size.
    pub const fn limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// Returns the target conversation.
    pub const fn conversation_id(self) -> ConversationId {
        self.conversation_id
    }

    /// Returns the exclusive upper cursor.
    pub const fn before_message_id(self) -> Option<MessageId> {
        self.before
    }

    /// Returns the requested page size.
    pub const fn page_size(self) -> usize {
        self.limit
    }
}

/// A page of messages ordered from newest to oldest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessagePage {
    messages: Vec<Message>,
    next_cursor: Option<MessageId>,
}

impl MessagePage {
    /// Creates a page returned by a read store.
    pub fn new(messages: Vec<Message>, next_cursor: Option<MessageId>) -> Self {
        Self {
            messages,
            next_cursor,
        }
    }

    /// Returns messages ordered from newest to oldest.
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Returns the cursor for the next older page, when one exists.
    pub const fn next_cursor(&self) -> Option<MessageId> {
        self.next_cursor
    }

    /// Consumes the page and returns its parts.
    pub fn into_parts(self) -> (Vec<Message>, Option<MessageId>) {
        (self.messages, self.next_cursor)
    }
}

/// Describes why listing messages failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ListMessagesError {
    /// The requested page size is outside `1..=MAX_MESSAGE_PAGE_SIZE`.
    InvalidPageSize {
        /// Rejected page size.
        value: usize,
        /// Maximum accepted page size.
        max: usize,
    },
    /// The conversation does not exist or is not visible to the actor.
    NotFound,
    /// The store returned a page inconsistent with the request.
    InvalidStoreResult,
    /// Persistence was temporarily unavailable.
    StoreUnavailable,
}

impl fmt::Display for ListMessagesError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPageSize { value, max } => {
                write!(
                    formatter,
                    "page size must be between 1 and {max}, got {value}"
                )
            }
            Self::NotFound => formatter.write_str("conversation not found"),
            Self::InvalidStoreResult => formatter.write_str("the store returned invalid state"),
            Self::StoreUnavailable => formatter.write_str("the store is unavailable"),
        }
    }
}

impl std::error::Error for ListMessagesError {}

impl<S: crate::ListMessagesStore> Chat<S> {
    /// Returns a validated page of messages visible to the requesting user.
    pub async fn list_messages(
        &self,
        actor_id: UserId,
        query: ListMessages,
    ) -> Result<MessagePage, ListMessagesError> {
        if !(1..=MAX_MESSAGE_PAGE_SIZE).contains(&query.limit) {
            return Err(ListMessagesError::InvalidPageSize {
                value: query.limit,
                max: MAX_MESSAGE_PAGE_SIZE,
            });
        }

        let page = self.store().list_messages(actor_id, query).await?;
        let messages = page.messages();
        let ids_descend = messages.windows(2).all(|pair| pair[0].id() > pair[1].id());
        let before_is_respected = query
            .before
            .is_none_or(|cursor| messages.iter().all(|message| message.id() < cursor));
        let cursor_is_valid = match (page.next_cursor(), messages.last()) {
            (None, _) => true,
            (Some(cursor), Some(last)) => cursor == last.id(),
            (Some(_), None) => false,
        };
        if messages.len() > query.limit
            || messages
                .iter()
                .any(|message| message.conversation_id() != query.conversation_id)
            || !ids_descend
            || !before_is_respected
            || !cursor_is_valid
        {
            return Err(ListMessagesError::InvalidStoreResult);
        }

        Ok(page)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        future::{Future, ready},
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
    };

    struct TrackingStore(Arc<AtomicBool>);

    impl crate::ListMessagesStore for TrackingStore {
        fn list_messages(
            &self,
            _actor_id: UserId,
            _query: ListMessages,
        ) -> impl Future<Output = Result<MessagePage, ListMessagesError>> + Send {
            self.0.store(true, Ordering::Relaxed);
            ready(Ok(MessagePage::new(Vec::new(), None)))
        }
    }

    #[test]
    fn invalid_page_size_is_rejected_before_reading() {
        let called = Arc::new(AtomicBool::new(false));
        let chat = Chat::new(TrackingStore(Arc::clone(&called)));
        let actor = UserId::new(1).expect("the fixture ID is valid");
        let conversation = ConversationId::new(2).expect("the fixture ID is valid");

        for value in [0, MAX_MESSAGE_PAGE_SIZE + 1] {
            let error = futures_executor::block_on(
                chat.list_messages(actor, ListMessages::new(conversation).limit(value)),
            )
            .expect_err("the page size is outside the accepted range");

            assert_eq!(
                error,
                ListMessagesError::InvalidPageSize {
                    value,
                    max: MAX_MESSAGE_PAGE_SIZE,
                }
            );
        }
        assert!(!called.load(Ordering::Relaxed));
    }
}
