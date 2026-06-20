use std::{future::Future, future::ready, time::SystemTime};

use chat::{
    AddMember, AddMemberError, AddMemberStore, Chat, ConversationId, CreateUser, CreateUserError,
    CreateUserStore, DisplayName, ListMessages, ListMessagesError, ListMessagesStore, Membership,
    MembershipRole, Message, MessageBody, MessageId, MessagePage, NewMembership, NewUser, User,
    UserId,
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
