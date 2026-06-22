use std::{fmt, sync::Arc};

use chat::{ConversationId, MessageId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ClientMessage {
    Subscribe { conversation_id: String },
    Unsubscribe { conversation_id: String },
}

impl ClientMessage {
    pub(crate) fn conversation_id(&self) -> Result<ConversationId, InvalidId> {
        let value = match self {
            Self::Subscribe { conversation_id } | Self::Unsubscribe { conversation_id } => {
                conversation_id
            }
        };
        parse_conversation_id(value)
    }

    pub(crate) fn raw_conversation_id(&self) -> &str {
        match self {
            Self::Subscribe { conversation_id } | Self::Unsubscribe { conversation_id } => {
                conversation_id
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SubscriptionRejection {
    InvalidRequest,
    NotFound,
    LimitReached,
    TemporarilyUnavailable,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum ServerMessage {
    Ready,
    Subscribed {
        conversation_id: String,
    },
    Unsubscribed {
        conversation_id: String,
    },
    SubscriptionRejected {
        conversation_id: String,
        reason: SubscriptionRejection,
    },
    ConversationCreated {
        conversation_id: String,
    },
    MessagePosted {
        conversation_id: String,
        message_id: String,
    },
}

impl ServerMessage {
    pub(crate) fn subscribed(conversation_id: ConversationId) -> Self {
        Self::Subscribed {
            conversation_id: conversation_id.to_string(),
        }
    }

    pub(crate) fn unsubscribed(conversation_id: ConversationId) -> Self {
        Self::Unsubscribed {
            conversation_id: conversation_id.to_string(),
        }
    }

    pub(crate) fn subscription_rejected(
        conversation_id: impl Into<String>,
        reason: SubscriptionRejection,
    ) -> Self {
        Self::SubscriptionRejected {
            conversation_id: conversation_id.into(),
            reason,
        }
    }

    pub(crate) fn conversation_created(conversation_id: ConversationId) -> Self {
        Self::ConversationCreated {
            conversation_id: conversation_id.to_string(),
        }
    }

    pub(crate) fn message_posted(conversation_id: ConversationId, message_id: MessageId) -> Self {
        Self::MessagePosted {
            conversation_id: conversation_id.to_string(),
            message_id: message_id.to_string(),
        }
    }
}

#[derive(Debug)]
pub(crate) enum ProtocolError {
    InvalidJson(serde_json::Error),
    Serialization(serde_json::Error),
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson(_) => formatter.write_str("invalid realtime protocol message"),
            Self::Serialization(_) => formatter.write_str("realtime protocol serialization failed"),
        }
    }
}

impl std::error::Error for ProtocolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidJson(error) | Self::Serialization(error) => Some(error),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InvalidId;

pub(crate) fn decode_client_message(value: &str) -> Result<ClientMessage, ProtocolError> {
    serde_json::from_str(value).map_err(ProtocolError::InvalidJson)
}

pub(crate) fn encode_server_message(message: &ServerMessage) -> Result<Arc<str>, ProtocolError> {
    serde_json::to_string(message)
        .map(Arc::<str>::from)
        .map_err(ProtocolError::Serialization)
}

fn parse_conversation_id(value: &str) -> Result<ConversationId, InvalidId> {
    value
        .parse::<i64>()
        .ok()
        .and_then(|value| ConversationId::new(value).ok())
        .ok_or(InvalidId)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_messages_are_strict_and_ids_are_positive() {
        let message = decode_client_message(r#"{"type":"subscribe","conversation_id":"42"}"#)
            .expect("valid subscribe message");
        assert_eq!(message.conversation_id().unwrap().get(), 42);

        for invalid in [
            r#"{"type":"subscribe","conversation_id":"0"}"#,
            r#"{"type":"subscribe","conversation_id":"-1"}"#,
            r#"{"type":"subscribe","conversation_id":"x"}"#,
            r#"{"type":"subscribe","conversation_id":"9223372036854775808"}"#,
        ] {
            assert!(
                decode_client_message(invalid)
                    .expect("shape is valid")
                    .conversation_id()
                    .is_err()
            );
        }
        assert!(
            decode_client_message(r#"{"type":"subscribe","conversation_id":"1","extra":true}"#)
                .is_err()
        );
        assert!(decode_client_message(r#"{"type":"unknown"}"#).is_err());
    }

    #[test]
    fn server_messages_use_string_ids_and_stable_tags() {
        let message = ServerMessage::message_posted(
            ConversationId::new(42).unwrap(),
            MessageId::new(99).unwrap(),
        );
        let encoded = encode_server_message(&message).unwrap();
        assert_eq!(
            encoded.as_ref(),
            r#"{"type":"message_posted","conversation_id":"42","message_id":"99"}"#
        );
    }
}
