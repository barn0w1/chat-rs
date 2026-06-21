use axum::{
    Json,
    http::{HeaderValue, header::CACHE_CONTROL},
    response::{IntoResponse, Response},
};
use chat::{
    ConversationDetails, ConversationMember, ConversationPage, ConversationSummary, MemberPage,
    MembershipRole, Message, MessagePage, User,
};
use serde::Serialize;

use crate::auth::unix_time_millis;

use super::problem::Problem;

#[derive(Debug, Serialize)]
pub(super) struct UserRepresentation {
    id: String,
    display_name: String,
    created_at_ms: i64,
}

impl UserRepresentation {
    pub(super) fn try_from_user(user: &User) -> Result<Self, Problem> {
        Ok(Self {
            id: user.id().get().to_string(),
            display_name: user.display_name().as_str().to_owned(),
            created_at_ms: timestamp_millis(user.created_at())?,
        })
    }
}

#[derive(Debug, Serialize)]
pub(super) struct SessionRepresentation {
    pub(super) user: UserRepresentation,
    pub(super) csrf_token: String,
}

#[derive(Debug, Serialize)]
struct ConversationRepresentation {
    id: String,
    title: String,
    created_at_ms: i64,
    role: &'static str,
}

impl ConversationRepresentation {
    fn try_from_summary(summary: &ConversationSummary) -> Result<Self, Problem> {
        let conversation = summary.conversation();
        Ok(Self {
            id: conversation.id().get().to_string(),
            title: conversation.title().as_str().to_owned(),
            created_at_ms: timestamp_millis(conversation.created_at())?,
            role: role_name(summary.role()),
        })
    }

    fn try_from_details(details: &ConversationDetails) -> Result<Self, Problem> {
        let conversation = details.conversation();
        Ok(Self {
            id: conversation.id().get().to_string(),
            title: conversation.title().as_str().to_owned(),
            created_at_ms: timestamp_millis(conversation.created_at())?,
            role: role_name(details.role()),
        })
    }
}

#[derive(Debug, Serialize)]
struct ConversationPageRepresentation {
    conversations: Vec<ConversationRepresentation>,
    next_cursor: Option<String>,
}

pub(super) fn conversation_page(page: &ConversationPage) -> Result<Response, Problem> {
    let conversations = page
        .conversations()
        .iter()
        .map(ConversationRepresentation::try_from_summary)
        .collect::<Result<_, _>>()?;
    Ok(json_no_store(ConversationPageRepresentation {
        conversations,
        next_cursor: page.next_cursor().map(|id| id.get().to_string()),
    }))
}

pub(super) fn conversation(details: &ConversationDetails) -> Result<Response, Problem> {
    Ok(json_no_store(ConversationRepresentation::try_from_details(
        details,
    )?))
}

#[derive(Debug, Serialize)]
struct MemberRepresentation {
    user: UserRepresentation,
    role: &'static str,
    joined_at_ms: i64,
}

impl MemberRepresentation {
    fn try_from_member(member: &ConversationMember) -> Result<Self, Problem> {
        Ok(Self {
            user: UserRepresentation::try_from_user(member.user())?,
            role: role_name(member.membership().role()),
            joined_at_ms: timestamp_millis(member.membership().joined_at())?,
        })
    }
}

#[derive(Debug, Serialize)]
struct MemberPageRepresentation {
    members: Vec<MemberRepresentation>,
    next_cursor: Option<String>,
}

pub(super) fn member_page(page: &MemberPage) -> Result<Response, Problem> {
    let members = page
        .members()
        .iter()
        .map(MemberRepresentation::try_from_member)
        .collect::<Result<_, _>>()?;
    Ok(json_no_store(MemberPageRepresentation {
        members,
        next_cursor: page.next_cursor().map(|id| id.get().to_string()),
    }))
}

#[derive(Debug, Serialize)]
struct MessageRepresentation {
    id: String,
    conversation_id: String,
    author_id: String,
    body: String,
    created_at_ms: i64,
}

impl MessageRepresentation {
    fn try_from_message(message: &Message) -> Result<Self, Problem> {
        Ok(Self {
            id: message.id().get().to_string(),
            conversation_id: message.conversation_id().get().to_string(),
            author_id: message.author_id().get().to_string(),
            body: message.body().as_str().to_owned(),
            created_at_ms: timestamp_millis(message.created_at())?,
        })
    }
}

#[derive(Debug, Serialize)]
struct MessagePageRepresentation {
    messages: Vec<MessageRepresentation>,
    next_cursor: Option<String>,
}

pub(super) fn message_page(page: &MessagePage) -> Result<Response, Problem> {
    let messages = page
        .messages()
        .iter()
        .map(MessageRepresentation::try_from_message)
        .collect::<Result<_, _>>()?;
    Ok(json_no_store(MessagePageRepresentation {
        messages,
        next_cursor: page.next_cursor().map(|id| id.get().to_string()),
    }))
}

pub(super) fn json_no_store<T: Serialize>(value: T) -> Response {
    let mut response = Json(value).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn role_name(role: MembershipRole) -> &'static str {
    match role {
        MembershipRole::Owner => "owner",
        MembershipRole::Member => "member",
    }
}

fn timestamp_millis(timestamp: &std::time::SystemTime) -> Result<i64, Problem> {
    unix_time_millis(*timestamp).map_err(|error| {
        tracing::error!(error = %error, "domain timestamp cannot be represented by the HTTP API");
        Problem::internal()
    })
}
