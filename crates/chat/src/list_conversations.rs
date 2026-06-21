use std::{collections::HashSet, fmt};

use crate::{Chat, Conversation, ConversationId, MembershipRole, UserId};

/// Default number of conversations requested in one page.
pub const DEFAULT_CONVERSATION_PAGE_SIZE: usize = 50;

/// Maximum number of conversations accepted in one page request.
pub const MAX_CONVERSATION_PAGE_SIZE: usize = 100;

/// Requests a page of conversations older than an optional cursor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ListConversations {
    before: Option<ConversationId>,
    limit: usize,
}

impl ListConversations {
    /// Creates a query using the default page size.
    pub const fn new() -> Self {
        Self {
            before: None,
            limit: DEFAULT_CONVERSATION_PAGE_SIZE,
        }
    }

    /// Sets the exclusive upper conversation-ID cursor.
    pub const fn before(mut self, conversation_id: ConversationId) -> Self {
        self.before = Some(conversation_id);
        self
    }

    /// Sets the requested page size.
    pub const fn limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// Returns the exclusive upper cursor.
    pub const fn before_conversation_id(self) -> Option<ConversationId> {
        self.before
    }

    /// Returns the requested page size.
    pub const fn page_size(self) -> usize {
        self.limit
    }
}

impl Default for ListConversations {
    fn default() -> Self {
        Self::new()
    }
}

/// A conversation entry visible to one user.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversationSummary {
    conversation: Conversation,
    role: MembershipRole,
}

impl ConversationSummary {
    /// Creates a summary returned by a read store.
    pub const fn new(conversation: Conversation, role: MembershipRole) -> Self {
        Self { conversation, role }
    }

    /// Returns the conversation.
    pub const fn conversation(&self) -> &Conversation {
        &self.conversation
    }

    /// Returns the user's role.
    pub const fn role(&self) -> MembershipRole {
        self.role
    }
}

/// A page of conversations ordered from newest to oldest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversationPage {
    conversations: Vec<ConversationSummary>,
    next_cursor: Option<ConversationId>,
}

impl ConversationPage {
    /// Creates a page returned by a read store.
    pub fn new(
        conversations: Vec<ConversationSummary>,
        next_cursor: Option<ConversationId>,
    ) -> Self {
        Self {
            conversations,
            next_cursor,
        }
    }

    /// Returns conversations ordered from newest to oldest.
    pub fn conversations(&self) -> &[ConversationSummary] {
        &self.conversations
    }

    /// Returns the cursor for the next older page, when one exists.
    pub const fn next_cursor(&self) -> Option<ConversationId> {
        self.next_cursor
    }

    /// Consumes the page and returns its parts.
    pub fn into_parts(self) -> (Vec<ConversationSummary>, Option<ConversationId>) {
        (self.conversations, self.next_cursor)
    }
}

/// Describes why listing conversations failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ListConversationsError {
    /// The requested page size is outside `1..=MAX_CONVERSATION_PAGE_SIZE`.
    InvalidPageSize {
        /// Rejected page size.
        value: usize,
        /// Maximum accepted page size.
        max: usize,
    },
    /// The store returned an invalid conversation page.
    InvalidStoreResult,
    /// Persistence was temporarily unavailable.
    StoreUnavailable,
}

impl fmt::Display for ListConversationsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPageSize { value, max } => {
                write!(
                    formatter,
                    "page size must be between 1 and {max}, got {value}"
                )
            }
            Self::InvalidStoreResult => formatter.write_str("the store returned invalid state"),
            Self::StoreUnavailable => formatter.write_str("the store is unavailable"),
        }
    }
}

impl std::error::Error for ListConversationsError {}

impl<S: crate::ListConversationsStore> Chat<S> {
    /// Returns a validated page of conversations visible to the requesting user.
    pub async fn list_conversations(
        &self,
        actor_id: UserId,
        query: ListConversations,
    ) -> Result<ConversationPage, ListConversationsError> {
        if !(1..=MAX_CONVERSATION_PAGE_SIZE).contains(&query.limit) {
            return Err(ListConversationsError::InvalidPageSize {
                value: query.limit,
                max: MAX_CONVERSATION_PAGE_SIZE,
            });
        }

        let page = self.store().list_conversations(actor_id, query).await?;
        let conversations = page.conversations();
        let mut ids = HashSet::with_capacity(conversations.len());
        let ids_descend = conversations
            .windows(2)
            .all(|pair| pair[0].conversation().id() > pair[1].conversation().id());
        let before_is_respected = query.before.is_none_or(|cursor| {
            conversations
                .iter()
                .all(|entry| entry.conversation().id() < cursor)
        });
        let cursor_is_valid = match (page.next_cursor(), conversations.last()) {
            (None, _) => true,
            (Some(cursor), Some(last)) => cursor == last.conversation().id(),
            (Some(_), None) => false,
        };
        if conversations.len() > query.limit
            || conversations
                .iter()
                .any(|entry| !ids.insert(entry.conversation().id()))
            || !ids_descend
            || !before_is_respected
            || !cursor_is_valid
        {
            return Err(ListConversationsError::InvalidStoreResult);
        }

        Ok(page)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        future::{Future, ready},
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
    };

    use super::*;

    struct TrackingStore(Arc<AtomicBool>);

    impl crate::ListConversationsStore for TrackingStore {
        fn list_conversations(
            &self,
            _actor_id: UserId,
            _query: ListConversations,
        ) -> impl Future<Output = Result<ConversationPage, ListConversationsError>> + Send {
            self.0.store(true, Ordering::Relaxed);
            ready(Ok(ConversationPage::new(Vec::new(), None)))
        }
    }

    #[test]
    fn invalid_page_size_is_rejected_before_reading() {
        let called = Arc::new(AtomicBool::new(false));
        let chat = Chat::new(TrackingStore(Arc::clone(&called)));
        let actor = UserId::new(1).expect("the fixture ID is valid");

        for value in [0, MAX_CONVERSATION_PAGE_SIZE + 1] {
            let error = futures_executor::block_on(
                chat.list_conversations(actor, ListConversations::new().limit(value)),
            )
            .expect_err("the page size is outside the accepted range");

            assert_eq!(
                error,
                ListConversationsError::InvalidPageSize {
                    value,
                    max: MAX_CONVERSATION_PAGE_SIZE,
                }
            );
        }
        assert!(!called.load(Ordering::Relaxed));
    }
}
