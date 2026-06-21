use std::{future::Future, future::ready, time::SystemTime};

use chat::{
    AddMember, AddMemberError, AddMemberStore, Chat, ChatEvent, Conversation, ConversationDetails,
    ConversationId, ConversationMember, ConversationPage, ConversationSummary, ConversationTitle,
    CreateConversation, CreateConversationError, CreateConversationStore, CreateUser,
    CreateUserError, CreateUserStore, CreatedConversation, DisplayName, GetConversationError,
    GetConversationStore, ListConversations, ListConversationsError, ListConversationsStore,
    ListMembers, ListMembersError, ListMembersStore, ListMessages, ListMessagesError,
    ListMessagesStore, MemberPage, Membership, MembershipRemoval, MembershipRole, Message,
    MessageBody, MessageId, MessagePage, NewConversation, NewMembership, NewUser, RemoveMember,
    RemoveMemberError, RemoveMemberStore, User, UserId,
};
use futures_executor::block_on;

fn user_id(value: i64) -> UserId {
    UserId::new(value).expect("the fixture user ID is valid")
}

fn conversation_id(value: i64) -> ConversationId {
    ConversationId::new(value).expect("the fixture conversation ID is valid")
}

fn user(value: i64, name: &str) -> User {
    User::new(
        user_id(value),
        DisplayName::try_from(name).expect("the fixture name is valid"),
        SystemTime::UNIX_EPOCH,
    )
}

fn conversation(value: i64, title: &str) -> Conversation {
    Conversation::new(
        conversation_id(value),
        ConversationTitle::try_from(title).expect("the fixture title is valid"),
        SystemTime::UNIX_EPOCH,
    )
}

fn membership(user: i64, role: MembershipRole) -> Membership {
    Membership::new(
        conversation_id(20),
        user_id(user),
        role,
        SystemTime::UNIX_EPOCH,
    )
}

struct UserWriter;

impl CreateUserStore for UserWriter {
    fn create_user(
        &self,
        user: NewUser,
    ) -> impl Future<Output = Result<User, CreateUserError>> + Send {
        ready(Ok(User::new(
            user_id(10),
            user.display_name().clone(),
            SystemTime::UNIX_EPOCH,
        )))
    }
}

#[test]
fn create_user_validates_persists_and_emits_an_event() {
    let result = block_on(Chat::new(UserWriter).create_user(CreateUser::new("Yuito")))
        .expect("the user should be created");

    assert_eq!(result.user().display_name().as_str(), "Yuito");
    assert_eq!(
        result.events(),
        &[ChatEvent::UserCreated {
            user: result.user().clone(),
        }]
    );
}

struct ConversationWriter {
    role: MembershipRole,
}

impl CreateConversationStore for ConversationWriter {
    fn create_conversation(
        &self,
        new: NewConversation,
    ) -> impl Future<Output = Result<CreatedConversation, CreateConversationError>> + Send {
        let conversation = Conversation::new(
            conversation_id(20),
            new.title().clone(),
            SystemTime::UNIX_EPOCH,
        );
        let membership = Membership::new(
            conversation.id(),
            new.creator_id(),
            self.role,
            SystemTime::UNIX_EPOCH,
        );
        ready(Ok(CreatedConversation::new(conversation, membership)))
    }
}

#[test]
fn create_conversation_requires_an_owner_membership() {
    let valid = block_on(
        Chat::new(ConversationWriter {
            role: MembershipRole::Owner,
        })
        .create_conversation(user_id(10), CreateConversation::new("General")),
    )
    .expect("the conversation should be created");

    assert_eq!(valid.conversation().title().as_str(), "General");
    assert_eq!(valid.creator_membership().role(), MembershipRole::Owner);
    assert_eq!(valid.events().len(), 1);

    let invalid = block_on(
        Chat::new(ConversationWriter {
            role: MembershipRole::Member,
        })
        .create_conversation(user_id(10), CreateConversation::new("General")),
    )
    .expect_err("the creator must be an owner");

    assert_eq!(invalid, CreateConversationError::InvalidStoreResult);
}

struct MembershipWriter;

impl AddMemberStore for MembershipWriter {
    fn add_member(
        &self,
        new: NewMembership,
    ) -> impl Future<Output = Result<Membership, AddMemberError>> + Send {
        assert_eq!(new.actor_id(), user_id(10));
        ready(Ok(Membership::new(
            new.conversation_id(),
            new.user_id(),
            MembershipRole::Member,
            SystemTime::UNIX_EPOCH,
        )))
    }
}

impl RemoveMemberStore for MembershipWriter {
    fn remove_member(
        &self,
        removal: MembershipRemoval,
    ) -> impl Future<Output = Result<Membership, RemoveMemberError>> + Send {
        assert_eq!(removal.actor_id(), user_id(10));
        ready(Ok(Membership::new(
            removal.conversation_id(),
            removal.user_id(),
            MembershipRole::Member,
            SystemTime::UNIX_EPOCH,
        )))
    }
}

#[test]
fn membership_mutations_return_the_changed_membership_and_event() {
    let chat = Chat::new(MembershipWriter);
    let added = block_on(chat.add_member(
        user_id(10),
        AddMember::new(conversation_id(20), user_id(11)),
    ))
    .expect("the member should be added");

    assert_eq!(added.membership().user_id(), user_id(11));
    assert!(matches!(added.events(), [ChatEvent::MemberAdded { .. }]));

    let removed = block_on(chat.remove_member(
        user_id(10),
        RemoveMember::new(conversation_id(20), user_id(11)),
    ))
    .expect("the member should be removed");

    assert_eq!(removed.membership().user_id(), user_id(11));
    assert!(matches!(
        removed.events(),
        [ChatEvent::MemberRemoved { .. }]
    ));
}

struct Reader;

impl GetConversationStore for Reader {
    fn get_conversation(
        &self,
        _actor_id: UserId,
        id: ConversationId,
    ) -> impl Future<Output = Result<ConversationDetails, GetConversationError>> + Send {
        ready(Ok(ConversationDetails::new(
            conversation(id.get(), "General"),
            MembershipRole::Owner,
        )))
    }
}

impl ListConversationsStore for Reader {
    fn list_conversations(
        &self,
        _actor_id: UserId,
        _query: ListConversations,
    ) -> impl Future<Output = Result<ConversationPage, ListConversationsError>> + Send {
        ready(Ok(ConversationPage::new(
            vec![ConversationSummary::new(
                conversation(20, "General"),
                MembershipRole::Owner,
            )],
            None,
        )))
    }
}

impl ListMembersStore for Reader {
    fn list_members(
        &self,
        _actor_id: UserId,
        _query: ListMembers,
    ) -> impl Future<Output = Result<MemberPage, ListMembersError>> + Send {
        ready(Ok(MemberPage::new(
            vec![
                ConversationMember::new(user(10, "Yuito"), membership(10, MembershipRole::Owner))
                    .expect("the fixture user and membership match"),
            ],
            None,
        )))
    }
}

impl ListMessagesStore for Reader {
    fn list_messages(
        &self,
        actor_id: UserId,
        query: ListMessages,
    ) -> impl Future<Output = Result<MessagePage, ListMessagesError>> + Send {
        let message = Message::new(
            MessageId::new(30).expect("the fixture message ID is valid"),
            query.conversation_id(),
            actor_id,
            MessageBody::try_from("hello").expect("the fixture body is valid"),
            SystemTime::UNIX_EPOCH,
        );
        ready(Ok(MessagePage::new(vec![message], None)))
    }
}

#[test]
fn read_use_cases_return_purpose_built_results() {
    let chat = Chat::new(Reader);
    let actor = user_id(10);
    let conversation_id = conversation_id(20);

    let details = block_on(chat.get_conversation(actor, conversation_id))
        .expect("the conversation should be visible");
    assert_eq!(details.role(), MembershipRole::Owner);

    let conversations = block_on(chat.list_conversations(actor, ListConversations::new()))
        .expect("conversations should be readable");
    assert_eq!(conversations.conversations().len(), 1);

    let members = block_on(chat.list_members(actor, ListMembers::new(conversation_id)))
        .expect("members should be readable");
    assert_eq!(members.members()[0].user().display_name().as_str(), "Yuito");

    let messages = block_on(chat.list_messages(actor, ListMessages::new(conversation_id)))
        .expect("messages should be readable");
    assert_eq!(messages.messages()[0].body().as_str(), "hello");
}
