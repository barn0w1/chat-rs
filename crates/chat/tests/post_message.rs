use std::{
    future::{Future, ready},
    sync::{Arc, Mutex},
    time::SystemTime,
};

use chat::{
    Chat, ChatEvent, ConversationId, Message, MessageBodyError, MessageId, NewMessage, PostMessage,
    PostMessageError, PostMessageStore, UserId,
};
use futures_executor::block_on;

#[derive(Clone, Copy)]
enum TestOutcome {
    Success,
    Failure(PostMessageError),
}

#[derive(Clone)]
struct TestStore {
    outcome: TestOutcome,
    received: Arc<Mutex<Vec<NewMessage>>>,
}

impl TestStore {
    fn successful() -> Self {
        Self::new(TestOutcome::Success)
    }

    fn failing(error: PostMessageError) -> Self {
        Self::new(TestOutcome::Failure(error))
    }

    fn new(outcome: TestOutcome) -> Self {
        Self {
            outcome,
            received: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn received(&self) -> Vec<NewMessage> {
        self.received
            .lock()
            .expect("the test store lock should not be poisoned")
            .clone()
    }
}

impl PostMessageStore for TestStore {
    fn create_message(
        &self,
        message: NewMessage,
    ) -> impl Future<Output = Result<Message, PostMessageError>> + Send {
        self.received
            .lock()
            .expect("the test store lock should not be poisoned")
            .push(message.clone());

        let result = match self.outcome {
            TestOutcome::Success => Ok(Message::new(
                MessageId::new(30).expect("the fixture ID is valid"),
                message.conversation_id(),
                message.author_id(),
                message.body().clone(),
                SystemTime::UNIX_EPOCH,
            )),
            TestOutcome::Failure(error) => Err(error),
        };

        ready(result)
    }
}

fn user_id() -> UserId {
    UserId::new(10).expect("the fixture ID is valid")
}

fn conversation_id() -> ConversationId {
    ConversationId::new(20).expect("the fixture ID is valid")
}

#[test]
fn posting_a_message_persists_it_before_producing_an_event() {
    let store = TestStore::successful();
    let chat = Chat::new(store.clone());

    let result =
        block_on(chat.post_message(user_id(), PostMessage::new(conversation_id(), "hello")))
            .expect("the message should be posted");

    let received = store.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].author_id(), user_id());
    assert_eq!(received[0].conversation_id(), conversation_id());
    assert_eq!(received[0].body().as_str(), "hello");
    assert_eq!(result.message().id().get(), 30);
    assert_eq!(
        result.events(),
        &[ChatEvent::MessagePosted {
            message: result.message().clone(),
        }]
    );
}

#[test]
fn invalid_bodies_are_rejected_before_the_store_is_called() {
    let store = TestStore::successful();
    let chat = Chat::new(store.clone());

    let error = block_on(chat.post_message(user_id(), PostMessage::new(conversation_id(), "   ")))
        .expect_err("a whitespace-only body should be rejected");

    assert_eq!(
        error,
        PostMessageError::InvalidBody(MessageBodyError::Empty)
    );
    assert!(store.received().is_empty());
}

#[test]
fn store_failures_are_exposed_as_use_case_errors() {
    let cases = [
        PostMessageError::ConversationNotFound,
        PostMessageError::AuthorNotMember,
        PostMessageError::StoreUnavailable,
    ];

    for expected in cases {
        let chat = Chat::new(TestStore::failing(expected));
        let error =
            block_on(chat.post_message(user_id(), PostMessage::new(conversation_id(), "hello")))
                .expect_err("the store failure should fail the use case");

        assert_eq!(error, expected);
    }
}
