use std::{collections::HashSet, fmt};

use crate::{Chat, ConversationId, Membership, User, UserId};

/// Default number of members requested in one page.
pub const DEFAULT_MEMBER_PAGE_SIZE: usize = 100;

/// Maximum number of members accepted in one page request.
pub const MAX_MEMBER_PAGE_SIZE: usize = 200;

/// Requests a page of conversation members after an optional user cursor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ListMembers {
    conversation_id: ConversationId,
    after: Option<UserId>,
    limit: usize,
}

impl ListMembers {
    /// Creates a query using the default page size.
    pub const fn new(conversation_id: ConversationId) -> Self {
        Self {
            conversation_id,
            after: None,
            limit: DEFAULT_MEMBER_PAGE_SIZE,
        }
    }

    /// Sets the exclusive lower user-ID cursor.
    pub const fn after(mut self, user_id: UserId) -> Self {
        self.after = Some(user_id);
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

    /// Returns the exclusive lower cursor.
    pub const fn after_user_id(self) -> Option<UserId> {
        self.after
    }

    /// Returns the requested page size.
    pub const fn page_size(self) -> usize {
        self.limit
    }
}

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

/// A page of conversation members ordered by user ID.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemberPage {
    members: Vec<ConversationMember>,
    next_cursor: Option<UserId>,
}

impl MemberPage {
    /// Creates a page returned by a read store.
    pub fn new(members: Vec<ConversationMember>, next_cursor: Option<UserId>) -> Self {
        Self {
            members,
            next_cursor,
        }
    }

    /// Returns members ordered by ascending user ID.
    pub fn members(&self) -> &[ConversationMember] {
        &self.members
    }

    /// Returns the cursor for the next page, when one exists.
    pub const fn next_cursor(&self) -> Option<UserId> {
        self.next_cursor
    }

    /// Consumes the page and returns its parts.
    pub fn into_parts(self) -> (Vec<ConversationMember>, Option<UserId>) {
        (self.members, self.next_cursor)
    }
}

/// Describes why listing members failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ListMembersError {
    /// The requested page size is outside `1..=MAX_MEMBER_PAGE_SIZE`.
    InvalidPageSize {
        /// Rejected page size.
        value: usize,
        /// Maximum accepted page size.
        max: usize,
    },
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

impl std::error::Error for ListMembersError {}

impl<S: crate::ListMembersStore> Chat<S> {
    /// Returns a validated member page visible to the requesting user.
    pub async fn list_members(
        &self,
        actor_id: UserId,
        query: ListMembers,
    ) -> Result<MemberPage, ListMembersError> {
        if !(1..=MAX_MEMBER_PAGE_SIZE).contains(&query.limit) {
            return Err(ListMembersError::InvalidPageSize {
                value: query.limit,
                max: MAX_MEMBER_PAGE_SIZE,
            });
        }

        let page = self.store().list_members(actor_id, query).await?;
        let members = page.members();
        let mut ids = HashSet::with_capacity(members.len());
        let ids_ascend = members
            .windows(2)
            .all(|pair| pair[0].user().id() < pair[1].user().id());
        let after_is_respected = query
            .after
            .is_none_or(|cursor| members.iter().all(|member| member.user().id() > cursor));
        let cursor_is_valid = match (page.next_cursor(), members.last()) {
            (None, _) => true,
            (Some(cursor), Some(last)) => cursor == last.user().id(),
            (Some(_), None) => false,
        };
        if members.len() > query.limit
            || members.iter().any(|member| {
                member.membership().conversation_id() != query.conversation_id
                    || !ids.insert(member.user().id())
            })
            || !ids_ascend
            || !after_is_respected
            || !cursor_is_valid
        {
            return Err(ListMembersError::InvalidStoreResult);
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

    impl crate::ListMembersStore for TrackingStore {
        fn list_members(
            &self,
            _actor_id: UserId,
            _query: ListMembers,
        ) -> impl Future<Output = Result<MemberPage, ListMembersError>> + Send {
            self.0.store(true, Ordering::Relaxed);
            ready(Ok(MemberPage::new(Vec::new(), None)))
        }
    }

    #[test]
    fn invalid_page_size_is_rejected_before_reading() {
        let called = Arc::new(AtomicBool::new(false));
        let chat = Chat::new(TrackingStore(Arc::clone(&called)));
        let actor = UserId::new(1).expect("the fixture actor ID is valid");
        let conversation = ConversationId::new(2).expect("the fixture conversation ID is valid");

        for value in [0, MAX_MEMBER_PAGE_SIZE + 1] {
            let error = futures_executor::block_on(
                chat.list_members(actor, ListMembers::new(conversation).limit(value)),
            )
            .expect_err("the page size is outside the accepted range");

            assert_eq!(
                error,
                ListMembersError::InvalidPageSize {
                    value,
                    max: MAX_MEMBER_PAGE_SIZE,
                }
            );
        }
        assert!(!called.load(Ordering::Relaxed));
    }
}
