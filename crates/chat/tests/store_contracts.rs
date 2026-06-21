use std::{future::Future, future::ready, time::SystemTime};

use chat::{
    AddMember, AddMemberError, AddMemberStore, Chat, ConversationId, CreateUser, CreateUserError,
    CreateUserStore, DisplayName, ListConversations, ListConversationsError,
    ListConversationsStore, ListMembers, ListMembersError, ListMembersStore, ListMessages,
    ListMessagesError, ListMessagesStore, Membership, MembershipRole, Message, MessageBody,
    MessageId, MessagePage, NewMembership, NewUser, User, UserId,
};
use futures_executor::block_on;

fn user_id(value: i64) -> UserId {
    UserId::new(value).expect("the fixture user ID is valid")
}

fn conversation_id(value: i64) -> ConversationId {
    ConversationId::new(value).expect("the fixture conversation ID is valid")
}

struct WrongUserStore;

impl CreateUserStore for WrongUserStore {
    fn create_user(
        &self,
        _user: NewUser,
    ) -> impl Future<Output = Result<User, CreateUserError>> + Send {
        ready(Ok(User::new(
            user_id(1),
            DisplayName::try_from("Different").expect("the fixture name is valid"),
            SystemTime::UNIX_EPOCH,
        )))
    }
}

#[test]
fn mutations_reject_inconsistent_store_results() {
    let error = block_on(Chat::new(WrongUserStore).create_user(CreateUser::new("Expected")))
        .expect_err("the returned display name does not match the command");

    assert_eq!(error, CreateUserError::InvalidStoreResult);
}

struct WrongMembershipStore;

impl AddMemberStore for WrongMembershipStore {
    fn add_member(
        &self,
        membership: NewMembership,
    ) -> impl Future<Output = Result<Membership, AddMemberError>> + Send {
        ready(Ok(Membership::new(
            membership.conversation_id(),
            user_id(999),
            MembershipRole::Member,
            SystemTime::UNIX_EPOCH,
        )))
    }
}

#[test]
fn membership_mutations_reject_the_wrong_target() {
    let error = block_on(
        Chat::new(WrongMembershipStore)
            .add_member(user_id(1), AddMember::new(conversation_id(2), user_id(3))),
    )
    .expect_err("the returned membership targets another user");

    assert_eq!(error, AddMemberError::InvalidStoreResult);
}

struct InvalidPageStore;

impl ListMessagesStore for InvalidPageStore {
    fn list_messages(
        &self,
        actor_id: UserId,
        query: ListMessages,
    ) -> impl Future<Output = Result<MessagePage, ListMessagesError>> + Send {
        let newer = Message::new(
            MessageId::new(10).expect("the fixture message ID is valid"),
            query.conversation_id(),
            actor_id,
            MessageBody::try_from("newer").expect("the fixture body is valid"),
            SystemTime::UNIX_EPOCH,
        );
        let older = Message::new(
            MessageId::new(11).expect("the fixture message ID is valid"),
            query.conversation_id(),
            actor_id,
            MessageBody::try_from("older").expect("the fixture body is valid"),
            SystemTime::UNIX_EPOCH,
        );
        ready(Ok(MessagePage::new(vec![newer, older], None)))
    }
}

#[test]
fn message_queries_reject_pages_that_are_not_newest_first() {
    let error = block_on(
        Chat::new(InvalidPageStore)
            .list_messages(user_id(1), ListMessages::new(conversation_id(2))),
    )
    .expect_err("message IDs must be strictly descending");

    assert_eq!(error, ListMessagesError::InvalidStoreResult);
}

struct InvalidConversationPageStore;

impl ListConversationsStore for InvalidConversationPageStore {
    fn list_conversations(
        &self,
        _actor_id: UserId,
        _query: ListConversations,
    ) -> impl Future<Output = Result<chat::ConversationPage, ListConversationsError>> + Send {
        let summary = |id| {
            chat::ConversationSummary::new(
                chat::Conversation::new(
                    conversation_id(id),
                    chat::ConversationTitle::try_from("General")
                        .expect("the fixture title is valid"),
                    SystemTime::UNIX_EPOCH,
                ),
                MembershipRole::Member,
            )
        };
        ready(Ok(chat::ConversationPage::new(
            vec![summary(10), summary(11)],
            None,
        )))
    }
}

#[test]
fn conversation_queries_reject_pages_that_are_not_newest_first() {
    let error = block_on(
        Chat::new(InvalidConversationPageStore)
            .list_conversations(user_id(1), ListConversations::new()),
    )
    .expect_err("conversation IDs must be strictly descending");

    assert_eq!(error, ListConversationsError::InvalidStoreResult);
}

struct InvalidMemberPageStore;

impl ListMembersStore for InvalidMemberPageStore {
    fn list_members(
        &self,
        _actor_id: UserId,
        query: ListMembers,
    ) -> impl Future<Output = Result<chat::MemberPage, ListMembersError>> + Send {
        let member = |id| {
            let user_id = user_id(id);
            chat::ConversationMember::new(
                User::new(
                    user_id,
                    DisplayName::try_from(format!("User {id}"))
                        .expect("the fixture display name is valid"),
                    SystemTime::UNIX_EPOCH,
                ),
                Membership::new(
                    query.conversation_id(),
                    user_id,
                    MembershipRole::Member,
                    SystemTime::UNIX_EPOCH,
                ),
            )
            .expect("the fixture user and membership match")
        };
        ready(Ok(chat::MemberPage::new(vec![member(2), member(1)], None)))
    }
}

#[test]
fn member_queries_reject_pages_that_are_not_in_user_id_order() {
    let error = block_on(
        Chat::new(InvalidMemberPageStore)
            .list_members(user_id(1), ListMembers::new(conversation_id(2))),
    )
    .expect_err("member IDs must be strictly ascending");

    assert_eq!(error, ListMembersError::InvalidStoreResult);
}
