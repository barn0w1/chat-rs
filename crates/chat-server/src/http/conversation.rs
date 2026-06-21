use axum::{
    Router,
    extract::{Path, Query, State, rejection::PathRejection},
    response::Response,
    routing::get,
};
use chat::{
    ConversationId, GetConversationError, ListConversations, ListConversationsError, ListMembers,
    ListMembersError, ListMessages, ListMessagesError, MAX_CONVERSATION_PAGE_SIZE,
    MAX_MEMBER_PAGE_SIZE, MAX_MESSAGE_PAGE_SIZE, MessageId, UserId,
};
use serde::Deserialize;

use crate::app::AppState;

use super::{authentication::AuthenticatedUser, problem::Problem, representation};

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/conversations", get(list_conversations))
        .route(
            "/api/v1/conversations/{conversation_id}",
            get(get_conversation),
        )
        .route(
            "/api/v1/conversations/{conversation_id}/members",
            get(list_members),
        )
        .route(
            "/api/v1/conversations/{conversation_id}/messages",
            get(list_messages),
        )
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConversationListQuery {
    before: Option<String>,
    limit: Option<usize>,
}

async fn list_conversations(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    query: Result<Query<ConversationListQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Response, Problem> {
    let Query(query) = query.map_err(|_| Problem::invalid_request())?;
    let mut request = ListConversations::new();
    if let Some(before) = query.before.as_deref() {
        request = request.before(parse_conversation_id(before)?);
    }
    if let Some(limit) = query.limit {
        request = request.limit(limit);
    }

    let page = state
        .chat
        .list_conversations(actor.user_id(), request)
        .await
        .map_err(map_list_conversations_error)?;
    representation::conversation_page(&page)
}

#[derive(Debug, Deserialize)]
struct ConversationPath {
    conversation_id: String,
}

async fn get_conversation(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    path: Result<Path<ConversationPath>, PathRejection>,
) -> Result<Response, Problem> {
    let conversation_id = conversation_id_from_path(path)?;
    let details = state
        .chat
        .get_conversation(actor.user_id(), conversation_id)
        .await
        .map_err(map_get_conversation_error)?;
    representation::conversation(&details)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MemberListQuery {
    after: Option<String>,
    limit: Option<usize>,
}

async fn list_members(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    path: Result<Path<ConversationPath>, PathRejection>,
    query: Result<Query<MemberListQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Response, Problem> {
    let conversation_id = conversation_id_from_path(path)?;
    let Query(query) = query.map_err(|_| Problem::invalid_request())?;
    let mut request = ListMembers::new(conversation_id);
    if let Some(after) = query.after.as_deref() {
        request = request.after(parse_user_id(after)?);
    }
    if let Some(limit) = query.limit {
        request = request.limit(limit);
    }

    let page = state
        .chat
        .list_members(actor.user_id(), request)
        .await
        .map_err(map_list_members_error)?;
    representation::member_page(&page)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MessageListQuery {
    before: Option<String>,
    limit: Option<usize>,
}

async fn list_messages(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    path: Result<Path<ConversationPath>, PathRejection>,
    query: Result<Query<MessageListQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Response, Problem> {
    let conversation_id = conversation_id_from_path(path)?;
    let Query(query) = query.map_err(|_| Problem::invalid_request())?;
    let mut request = ListMessages::new(conversation_id);
    if let Some(before) = query.before.as_deref() {
        request = request.before(parse_message_id(before)?);
    }
    if let Some(limit) = query.limit {
        request = request.limit(limit);
    }

    let page = state
        .chat
        .list_messages(actor.user_id(), request)
        .await
        .map_err(map_list_messages_error)?;
    representation::message_page(&page)
}

fn conversation_id_from_path(
    path: Result<Path<ConversationPath>, PathRejection>,
) -> Result<ConversationId, Problem> {
    let Path(path) = path.map_err(|_| Problem::invalid_request())?;
    parse_conversation_id(&path.conversation_id)
}

fn parse_conversation_id(value: &str) -> Result<ConversationId, Problem> {
    value
        .parse::<i64>()
        .ok()
        .and_then(|value| ConversationId::new(value).ok())
        .ok_or(Problem::invalid_request())
}

fn parse_user_id(value: &str) -> Result<UserId, Problem> {
    value
        .parse::<i64>()
        .ok()
        .and_then(|value| UserId::new(value).ok())
        .ok_or(Problem::invalid_request())
}

fn parse_message_id(value: &str) -> Result<MessageId, Problem> {
    value
        .parse::<i64>()
        .ok()
        .and_then(|value| MessageId::new(value).ok())
        .ok_or(Problem::invalid_request())
}

fn map_get_conversation_error(error: GetConversationError) -> Problem {
    match error {
        GetConversationError::NotFound => Problem::not_found(),
        GetConversationError::StoreUnavailable => {
            tracing::error!("conversation store was unavailable");
            Problem::unavailable()
        }
        GetConversationError::InvalidStoreResult => {
            tracing::error!("conversation store returned invalid state");
            Problem::internal()
        }
    }
}

fn map_list_conversations_error(error: ListConversationsError) -> Problem {
    match error {
        ListConversationsError::InvalidPageSize { .. } => {
            Problem::validation("limit", MAX_CONVERSATION_PAGE_SIZE)
        }
        ListConversationsError::StoreUnavailable => {
            tracing::error!("conversation store was unavailable");
            Problem::unavailable()
        }
        ListConversationsError::InvalidStoreResult => {
            tracing::error!("conversation store returned invalid state");
            Problem::internal()
        }
    }
}

fn map_list_members_error(error: ListMembersError) -> Problem {
    match error {
        ListMembersError::InvalidPageSize { .. } => {
            Problem::validation("limit", MAX_MEMBER_PAGE_SIZE)
        }
        ListMembersError::NotFound => Problem::not_found(),
        ListMembersError::StoreUnavailable => {
            tracing::error!("member store was unavailable");
            Problem::unavailable()
        }
        ListMembersError::InvalidStoreResult => {
            tracing::error!("member store returned invalid state");
            Problem::internal()
        }
    }
}

fn map_list_messages_error(error: ListMessagesError) -> Problem {
    match error {
        ListMessagesError::InvalidPageSize { .. } => {
            Problem::validation("limit", MAX_MESSAGE_PAGE_SIZE)
        }
        ListMessagesError::NotFound => Problem::not_found(),
        ListMessagesError::StoreUnavailable => {
            tracing::error!("message store was unavailable");
            Problem::unavailable()
        }
        ListMessagesError::InvalidStoreResult => {
            tracing::error!("message store returned invalid state");
            Problem::internal()
        }
    }
}
