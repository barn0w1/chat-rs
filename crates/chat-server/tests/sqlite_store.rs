use chat::{
    AddMember, AddMemberError, Chat, CreateConversation, CreateConversationError, CreateUser,
    GetConversationError, GetMessageError, ListConversations, ListMembers, ListMembersError,
    ListMessages, ListMessagesError, MembershipRole, PostMessage, PostMessageError, RemoveMember,
    RemoveMemberError,
};
use chat_server::sqlite::SqliteStore;
use tempfile::TempDir;

struct TestDatabase {
    _directory: TempDir,
    path: std::path::PathBuf,
    store: SqliteStore,
}

impl TestDatabase {
    async fn open() -> Self {
        let directory = tempfile::tempdir().expect("temporary directory can be created");
        let path = directory.path().join("chat.sqlite3");
        let store = SqliteStore::open(&path)
            .await
            .expect("temporary database can be opened");
        Self {
            _directory: directory,
            path,
            store,
        }
    }
}

#[tokio::test]
async fn complete_workflow_persists_and_queries_chat_state() {
    let database = TestDatabase::open().await;
    let chat = Chat::new(database.store.clone());
    let owner = chat
        .create_user(CreateUser::new("Owner"))
        .await
        .expect("owner can be created")
        .user()
        .id();
    let member = chat
        .create_user(CreateUser::new("Member"))
        .await
        .expect("member can be created")
        .user()
        .id();
    let outsider = chat
        .create_user(CreateUser::new("Outsider"))
        .await
        .expect("outsider can be created")
        .user()
        .id();

    let conversation = chat
        .create_conversation(owner, CreateConversation::new("General"))
        .await
        .expect("conversation can be created")
        .conversation()
        .id();
    chat.add_member(owner, AddMember::new(conversation, member))
        .await
        .expect("owner can add a member");

    let details = chat
        .get_conversation(member, conversation)
        .await
        .expect("member can read the conversation");
    assert_eq!(details.role(), MembershipRole::Member);
    assert_eq!(
        chat.get_conversation(outsider, conversation).await,
        Err(GetConversationError::NotFound)
    );

    let conversations = chat
        .list_conversations(member, ListConversations::new())
        .await
        .expect("visible conversations can be listed");
    assert_eq!(conversations.conversations().len(), 1);
    assert_eq!(
        conversations.conversations()[0].conversation().id(),
        conversation
    );

    let members = chat
        .list_members(member, ListMembers::new(conversation))
        .await
        .expect("members can list conversation members");
    assert_eq!(members.members().len(), 2);
    assert_eq!(
        members.members()[0].membership().role(),
        MembershipRole::Owner
    );
    assert_eq!(
        members.members()[1].membership().role(),
        MembershipRole::Member
    );

    let empty_page = chat
        .list_messages(member, ListMessages::new(conversation))
        .await
        .expect("a visible conversation can have no messages");
    assert!(empty_page.messages().is_empty());
    assert_eq!(
        chat.list_messages(outsider, ListMessages::new(conversation))
            .await,
        Err(ListMessagesError::NotFound)
    );

    let mut posted = Vec::new();
    for body in ["first", "second", "third"] {
        posted.push(
            chat.post_message(member, PostMessage::new(conversation, body))
                .await
                .expect("member can post a message")
                .message()
                .id(),
        );
    }

    let message = chat
        .get_message(member, conversation, posted[1])
        .await
        .expect("a member can read one message");
    assert_eq!(message.body().as_str(), "second");
    assert_eq!(
        chat.get_message(outsider, conversation, posted[1]).await,
        Err(GetMessageError::NotFound)
    );
    let other_conversation = chat
        .create_conversation(owner, CreateConversation::new("Other"))
        .await
        .expect("another conversation can be created")
        .conversation()
        .id();
    assert_eq!(
        chat.get_message(owner, other_conversation, posted[1]).await,
        Err(GetMessageError::NotFound)
    );
    let missing_message = chat::MessageId::new(i64::MAX).expect("fixture ID is positive");
    assert_eq!(
        chat.get_message(owner, conversation, missing_message).await,
        Err(GetMessageError::NotFound)
    );

    let first_page = chat
        .list_messages(member, ListMessages::new(conversation).limit(2))
        .await
        .expect("first message page can be read");
    let first_page_bodies = first_page
        .messages()
        .iter()
        .map(|message| message.body().as_str())
        .collect::<Vec<_>>();
    assert_eq!(first_page_bodies, ["third", "second"]);
    let cursor = first_page.next_cursor().expect("an older page exists");

    let second_page = chat
        .list_messages(
            member,
            ListMessages::new(conversation).before(cursor).limit(2),
        )
        .await
        .expect("second message page can be read");
    assert_eq!(second_page.messages().len(), 1);
    assert_eq!(second_page.messages()[0].body().as_str(), "first");
    assert_eq!(second_page.next_cursor(), None);

    chat.remove_member(owner, RemoveMember::new(conversation, member))
        .await
        .expect("owner can remove the member");
    assert_eq!(
        chat.post_message(member, PostMessage::new(conversation, "denied"))
            .await,
        Err(PostMessageError::AuthorNotMember)
    );
    assert_eq!(
        chat.list_members(member, ListMembers::new(conversation))
            .await,
        Err(ListMembersError::NotFound)
    );

    drop(chat);
    database.store.close().await;
    let reopened = SqliteStore::open(&database.path)
        .await
        .expect("database can be reopened and migrated again");
    let reopened_chat = Chat::new(reopened.clone());
    let messages = reopened_chat
        .list_messages(owner, ListMessages::new(conversation))
        .await
        .expect("committed messages survive reopening");
    assert_eq!(messages.messages().len(), 3);
    assert_eq!(
        reopened_chat
            .get_message(owner, conversation, posted[0])
            .await
            .expect("a committed message survives reopening")
            .body()
            .as_str(),
        "first"
    );
    reopened.close().await;
}

#[tokio::test]
async fn mutations_map_expected_domain_errors() {
    let database = TestDatabase::open().await;
    let chat = Chat::new(database.store.clone());
    let owner = chat
        .create_user(CreateUser::new("Owner"))
        .await
        .expect("owner can be created")
        .user()
        .id();
    let member = chat
        .create_user(CreateUser::new("Member"))
        .await
        .expect("member can be created")
        .user()
        .id();
    let missing = chat::UserId::new(i64::MAX).expect("fixture ID is positive");

    assert_eq!(
        chat.create_conversation(missing, CreateConversation::new("Missing creator"))
            .await,
        Err(CreateConversationError::CreatorNotFound)
    );

    let conversation = chat
        .create_conversation(owner, CreateConversation::new("General"))
        .await
        .expect("conversation can be created")
        .conversation()
        .id();
    let missing_conversation = chat::ConversationId::new(i64::MAX).expect("fixture ID is positive");

    assert_eq!(
        chat.add_member(owner, AddMember::new(missing_conversation, member),)
            .await,
        Err(AddMemberError::ConversationNotFound)
    );
    assert_eq!(
        chat.add_member(owner, AddMember::new(conversation, missing))
            .await,
        Err(AddMemberError::UserNotFound)
    );

    chat.add_member(owner, AddMember::new(conversation, member))
        .await
        .expect("member can be added once");
    assert_eq!(
        chat.add_member(owner, AddMember::new(conversation, member))
            .await,
        Err(AddMemberError::AlreadyMember)
    );
    assert_eq!(
        chat.add_member(member, AddMember::new(conversation, missing))
            .await,
        Err(AddMemberError::NotOwner)
    );
    assert_eq!(
        chat.remove_member(member, RemoveMember::new(conversation, owner))
            .await,
        Err(RemoveMemberError::NotAuthorized)
    );
    assert_eq!(
        chat.remove_member(owner, RemoveMember::new(conversation, owner))
            .await,
        Err(RemoveMemberError::OwnerCannotLeave)
    );
    assert_eq!(
        chat.post_message(owner, PostMessage::new(missing_conversation, "missing"))
            .await,
        Err(PostMessageError::ConversationNotFound)
    );

    database.store.close().await;
}

#[tokio::test]
async fn competing_add_member_operations_are_atomic() {
    let database = TestDatabase::open().await;
    let chat = Chat::new(database.store.clone());
    let owner = chat
        .create_user(CreateUser::new("Owner"))
        .await
        .expect("owner can be created")
        .user()
        .id();
    let member = chat
        .create_user(CreateUser::new("Member"))
        .await
        .expect("member can be created")
        .user()
        .id();
    let conversation = chat
        .create_conversation(owner, CreateConversation::new("General"))
        .await
        .expect("conversation can be created")
        .conversation()
        .id();

    let command = AddMember::new(conversation, member);
    let (left, right) = tokio::join!(
        chat.add_member(owner, command),
        chat.add_member(owner, command)
    );
    let mut successes = 0;
    let mut duplicate_errors = 0;
    for result in [left, right] {
        match result {
            Ok(_) => successes += 1,
            Err(AddMemberError::AlreadyMember) => duplicate_errors += 1,
            Err(error) => panic!("unexpected competing mutation error: {error}"),
        }
    }
    assert_eq!(successes, 1);
    assert_eq!(duplicate_errors, 1);

    let members = chat
        .list_members(owner, ListMembers::new(conversation))
        .await
        .expect("membership state remains valid");
    assert_eq!(members.members().len(), 2);
    database.store.close().await;
}

#[tokio::test]
async fn conversation_and_member_pages_use_stable_keyset_cursors() {
    let database = TestDatabase::open().await;
    let chat = Chat::new(database.store.clone());
    let owner = chat
        .create_user(CreateUser::new("Owner"))
        .await
        .expect("owner can be created")
        .user()
        .id();
    let first_member = chat
        .create_user(CreateUser::new("First member"))
        .await
        .expect("member can be created")
        .user()
        .id();
    let second_member = chat
        .create_user(CreateUser::new("Second member"))
        .await
        .expect("member can be created")
        .user()
        .id();

    let mut conversation_ids = Vec::new();
    for title in ["First", "Second", "Third"] {
        conversation_ids.push(
            chat.create_conversation(owner, CreateConversation::new(title))
                .await
                .expect("conversation can be created")
                .conversation()
                .id(),
        );
    }
    let target = conversation_ids[2];
    chat.add_member(owner, AddMember::new(target, first_member))
        .await
        .expect("member can be added");
    chat.add_member(owner, AddMember::new(target, second_member))
        .await
        .expect("member can be added");

    let first_page = chat
        .list_conversations(owner, ListConversations::new().limit(2))
        .await
        .expect("first page can be read");
    assert_eq!(
        first_page
            .conversations()
            .iter()
            .map(|entry| entry.conversation().id())
            .collect::<Vec<_>>(),
        [conversation_ids[2], conversation_ids[1]]
    );
    let cursor = first_page.next_cursor().expect("an older page exists");
    let second_page = chat
        .list_conversations(owner, ListConversations::new().before(cursor).limit(2))
        .await
        .expect("second page can be read");
    assert_eq!(
        second_page.conversations()[0].conversation().id(),
        conversation_ids[0]
    );
    assert_eq!(second_page.next_cursor(), None);

    let first_page = chat
        .list_members(owner, ListMembers::new(target).limit(2))
        .await
        .expect("first member page can be read");
    assert_eq!(
        first_page
            .members()
            .iter()
            .map(|member| member.user().id())
            .collect::<Vec<_>>(),
        [owner, first_member]
    );
    let cursor = first_page.next_cursor().expect("another member exists");
    let second_page = chat
        .list_members(owner, ListMembers::new(target).after(cursor).limit(2))
        .await
        .expect("second member page can be read");
    assert_eq!(second_page.members()[0].user().id(), second_member);
    assert_eq!(second_page.next_cursor(), None);

    database.store.close().await;
}
