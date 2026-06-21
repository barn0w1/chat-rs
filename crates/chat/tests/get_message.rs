use std::{future::Future, future::ready, time::SystemTime};

use chat::{
    Chat, ConversationId, GetMessageError, GetMessageStore, Message, MessageBody, MessageId, UserId,
};
use futures_executor::block_on;

#[derive(Clone, Copy)]
enum ResultShape {
    Matching,
    WrongMessage,
    WrongConversation,
    Failure(GetMessageError),
}

struct TestStore(ResultShape);

impl GetMessageStore for TestStore {
    fn get_message(
        &self,
        actor_id: UserId,
        conversation_id: ConversationId,
        message_id: MessageId,
    ) -> impl Future<Output = Result<Message, GetMessageError>> + Send {
        let result = match self.0 {
            ResultShape::Matching => Ok(message(message_id, conversation_id, actor_id)),
            ResultShape::WrongMessage => Ok(message(
                MessageId::new(message_id.get() + 1).expect("the fixture message ID is valid"),
                conversation_id,
                actor_id,
            )),
            ResultShape::WrongConversation => Ok(message(
                message_id,
                ConversationId::new(conversation_id.get() + 1)
                    .expect("the fixture conversation ID is valid"),
                actor_id,
            )),
            ResultShape::Failure(error) => Err(error),
        };
        ready(result)
    }
}

fn message(id: MessageId, conversation_id: ConversationId, author_id: UserId) -> Message {
    Message::new(
        id,
        conversation_id,
        author_id,
        MessageBody::try_from("hello").expect("the fixture body is valid"),
        SystemTime::UNIX_EPOCH,
    )
}

fn ids() -> (UserId, ConversationId, MessageId) {
    (
        UserId::new(1).expect("the fixture user ID is valid"),
        ConversationId::new(2).expect("the fixture conversation ID is valid"),
        MessageId::new(3).expect("the fixture message ID is valid"),
    )
}

#[test]
fn matching_message_is_returned() {
    let (actor, conversation, message) = ids();

    let result = block_on(Chat::new(TestStore(ResultShape::Matching)).get_message(
        actor,
        conversation,
        message,
    ))
    .expect("the message should be returned");

    assert_eq!(result.id(), message);
    assert_eq!(result.conversation_id(), conversation);
}

#[test]
fn inconsistent_store_results_are_rejected() {
    let (actor, conversation, message) = ids();

    for shape in [ResultShape::WrongMessage, ResultShape::WrongConversation] {
        let error = block_on(Chat::new(TestStore(shape)).get_message(actor, conversation, message))
            .expect_err("the returned message does not match the request");

        assert_eq!(error, GetMessageError::InvalidStoreResult);
    }
}

#[test]
fn store_failures_are_preserved() {
    let (actor, conversation, message) = ids();

    for expected in [GetMessageError::NotFound, GetMessageError::StoreUnavailable] {
        let error = block_on(
            Chat::new(TestStore(ResultShape::Failure(expected))).get_message(
                actor,
                conversation,
                message,
            ),
        )
        .expect_err("the store failure should fail the use case");

        assert_eq!(error, expected);
    }
}
