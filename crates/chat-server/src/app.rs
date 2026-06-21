use std::time::Duration;

use axum::{Router, extract::State, http::StatusCode, routing::get};
use chat::Chat;
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::{
    AdmissionMode, Config,
    auth::{AuthStore, CookiePolicy, OidcProvider},
    http,
    sqlite::SqliteStore,
};

const READINESS_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Clone, Debug)]
pub(crate) struct AppState {
    pub(crate) store: SqliteStore,
    pub(crate) chat: Chat<SqliteStore>,
    pub(crate) auth: AuthStore,
    pub(crate) cookies: CookiePolicy,
    pub(crate) expected_origin: String,
    pub(crate) oidc: Option<OidcProvider>,
    pub(crate) admission_mode: AdmissionMode,
}

pub(crate) fn router(store: SqliteStore, config: &Config, oidc: Option<OidcProvider>) -> Router {
    let oidc_enabled = oidc.is_some();
    let state = AppState {
        chat: Chat::new(store.clone()),
        auth: AuthStore::new(store.clone()),
        store,
        cookies: CookiePolicy::new(config.public_url()),
        expected_origin: config.public_url().origin().ascii_serialization(),
        oidc,
        admission_mode: config.admission_mode(),
    };
    let trace = TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
        .on_request(DefaultOnRequest::new().level(Level::DEBUG))
        .on_response(DefaultOnResponse::new().level(Level::INFO));

    Router::new()
        .route("/health/live", get(liveness))
        .route("/health/ready", get(readiness))
        .merge(http::routes(oidc_enabled))
        .layer(trace)
        .with_state(state)
}

async fn liveness() -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn readiness(State(state): State<AppState>) -> StatusCode {
    match tokio::time::timeout(READINESS_TIMEOUT, state.store.check_readiness()).await {
        Ok(Ok(())) => StatusCode::NO_CONTENT,
        Ok(Err(_)) | Err(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use axum::{
        body::{Body, to_bytes},
        http::{
            Method, Request,
            header::{
                CACHE_CONTROL, CONTENT_TYPE, COOKIE, LOCATION, ORIGIN, SET_COOKIE, WWW_AUTHENTICATE,
            },
        },
        response::Response,
    };
    use chat::{
        AddMember, CreateConversation, CreateUser, ListConversations, ListMessages,
        MAX_CONVERSATION_PAGE_SIZE, MAX_CONVERSATION_TITLE_CHARS, MAX_MESSAGE_BODY_CHARS,
        PostMessage,
    };
    use serde_json::Value;
    use tempfile::TempDir;
    use tower::ServiceExt;

    use crate::config::ConfigValues;

    use super::*;

    async fn test_app() -> (Router, SqliteStore, TempDir) {
        let directory = tempfile::tempdir().expect("temporary directory can be created");
        let store = SqliteStore::open(directory.path().join("chat.sqlite3"))
            .await
            .expect("test database can be opened");
        let config = Config::from_values(ConfigValues {
            database_path: Some(directory.path().join("chat.sqlite3").into_os_string()),
            ..ConfigValues::default()
        })
        .expect("test configuration is valid");
        (router(store.clone(), &config, None), store, directory)
    }

    async fn response_status(app: Router, method: Method, uri: &str) -> StatusCode {
        app.oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .body(Body::empty())
                .expect("test request is valid"),
        )
        .await
        .expect("router is infallible")
        .status()
    }

    async fn request(app: Router, request: Request<Body>) -> Response {
        app.oneshot(request).await.expect("router is infallible")
    }

    async fn response_json(response: Response) -> Value {
        let body = to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("response body can be read");
        serde_json::from_slice(&body).expect("response is JSON")
    }

    async fn session_cookie(store: &SqliteStore, user_id: chat::UserId) -> String {
        session_credentials(store, user_id).await.0
    }

    async fn session_credentials(store: &SqliteStore, user_id: chat::UserId) -> (String, String) {
        let now = SystemTime::now();
        let issued = AuthStore::new(store.clone())
            .issue_session(user_id, None, now)
            .await
            .expect("test session can be issued");
        let token = issued.session_token();
        let csrf = AuthStore::new(store.clone())
            .resolve_session(&token, now)
            .await
            .expect("test session can be resolved")
            .expect("test session exists")
            .csrf_token();
        (format!("chat_session={token}"), csrf)
    }

    fn authenticated_get(uri: &str, cookie: &str) -> Request<Body> {
        Request::builder()
            .uri(uri)
            .header(COOKIE, cookie)
            .body(Body::empty())
            .expect("test request is valid")
    }

    fn authenticated_json_post(
        uri: &str,
        cookie: &str,
        csrf: &str,
        body: impl Into<Body>,
    ) -> Request<Body> {
        Request::builder()
            .method(Method::POST)
            .uri(uri)
            .header(CONTENT_TYPE, "application/json")
            .header(COOKIE, cookie)
            .header(ORIGIN, "http://127.0.0.1:3000")
            .header("x-csrf-token", csrf)
            .body(body.into())
            .expect("test request is valid")
    }

    #[tokio::test]
    async fn health_routes_report_process_and_database_state() {
        let (app, store, _directory) = test_app().await;

        assert_eq!(
            response_status(app.clone(), Method::GET, "/health/live").await,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            response_status(app.clone(), Method::GET, "/health/ready").await,
            StatusCode::NO_CONTENT
        );
        store.close().await;
        assert_eq!(
            response_status(app.clone(), Method::GET, "/health/live").await,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            response_status(app, Method::GET, "/health/ready").await,
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[tokio::test]
    async fn router_preserves_standard_and_optional_oidc_responses() {
        let (app, store, _directory) = test_app().await;

        assert_eq!(
            response_status(app.clone(), Method::GET, "/missing").await,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            response_status(app.clone(), Method::POST, "/health/live").await,
            StatusCode::METHOD_NOT_ALLOWED
        );
        assert_eq!(
            response_status(app, Method::GET, "/auth/oidc/start").await,
            StatusCode::NOT_FOUND
        );
        store.close().await;
    }

    #[tokio::test]
    async fn session_resource_returns_csrf_and_logout_requires_it() {
        let (app, store, _directory) = test_app().await;
        let user_id = Chat::new(store.clone())
            .create_user(CreateUser::new("Alice"))
            .await
            .expect("user can be created")
            .user()
            .id();
        let auth = AuthStore::new(store.clone());
        let now = SystemTime::now();
        let issued = auth
            .issue_session(user_id, None, now)
            .await
            .expect("session can be issued");
        let cookie = format!("chat_session={}", issued.session_token());
        let csrf = auth
            .resolve_session(&issued.session_token(), now)
            .await
            .expect("session can be read")
            .expect("session exists")
            .csrf_token();

        let response = request(app.clone(), authenticated_get("/api/v1/session", &cookie)).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
        assert_eq!(
            response_json(response).await.pointer("/user/id"),
            Some(&Value::from(user_id.to_string()))
        );

        let rejected = request(
            app.clone(),
            Request::builder()
                .method(Method::DELETE)
                .uri("/api/v1/session")
                .header(COOKIE, &cookie)
                .body(Body::empty())
                .expect("test request is valid"),
        )
        .await;
        assert_eq!(rejected.status(), StatusCode::FORBIDDEN);

        let logged_out = request(
            app,
            Request::builder()
                .method(Method::DELETE)
                .uri("/api/v1/session")
                .header(COOKIE, cookie)
                .header(ORIGIN, "http://127.0.0.1:3000")
                .header("x-csrf-token", csrf)
                .body(Body::empty())
                .expect("test request is valid"),
        )
        .await;
        assert_eq!(logged_out.status(), StatusCode::NO_CONTENT);
        assert!(logged_out.headers().contains_key(SET_COOKIE));
        store.close().await;
    }

    #[tokio::test]
    async fn authenticated_chat_reads_are_paginated_and_hide_invisible_resources() {
        let (app, store, _directory) = test_app().await;
        let chat = Chat::new(store.clone());
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
        let older = chat
            .create_conversation(owner, CreateConversation::new("Older"))
            .await
            .expect("conversation can be created")
            .conversation()
            .id();
        let newer = chat
            .create_conversation(owner, CreateConversation::new("Newer"))
            .await
            .expect("conversation can be created")
            .conversation()
            .id();
        chat.add_member(owner, AddMember::new(newer, member))
            .await
            .expect("member can be added");
        chat.post_message(owner, PostMessage::new(newer, "first"))
            .await
            .expect("message can be posted");
        let newest_message = chat
            .post_message(member, PostMessage::new(newer, "second"))
            .await
            .expect("message can be posted")
            .message()
            .id();
        let owner_cookie = session_cookie(&store, owner).await;
        let outsider_cookie = session_cookie(&store, outsider).await;

        let response = request(
            app.clone(),
            authenticated_get("/api/v1/conversations?limit=1", &owner_cookie),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
        let page = response_json(response).await;
        assert_eq!(page["conversations"][0]["id"], newer.to_string());
        assert_eq!(page["next_cursor"], newer.to_string());

        let response = request(
            app.clone(),
            authenticated_get(
                &format!("/api/v1/conversations?before={newer}&limit=1"),
                &owner_cookie,
            ),
        )
        .await;
        let page = response_json(response).await;
        assert_eq!(page["conversations"][0]["id"], older.to_string());
        assert_eq!(page["next_cursor"], Value::Null);

        let response = request(
            app.clone(),
            authenticated_get(&format!("/api/v1/conversations/{newer}"), &owner_cookie),
        )
        .await;
        let details = response_json(response).await;
        assert_eq!(details["title"], "Newer");
        assert_eq!(details["role"], "owner");

        let response = request(
            app.clone(),
            authenticated_get(
                &format!("/api/v1/conversations/{newer}/members?limit=1"),
                &owner_cookie,
            ),
        )
        .await;
        let members = response_json(response).await;
        assert_eq!(members["members"][0]["user"]["id"], owner.to_string());
        assert_eq!(members["next_cursor"], owner.to_string());

        let response = request(
            app.clone(),
            authenticated_get(
                &format!("/api/v1/conversations/{newer}/messages?limit=1"),
                &owner_cookie,
            ),
        )
        .await;
        let messages = response_json(response).await;
        assert_eq!(messages["messages"][0]["id"], newest_message.to_string());
        assert_eq!(messages["messages"][0]["body"], "second");
        assert_eq!(messages["next_cursor"], newest_message.to_string());

        let hidden = request(
            app,
            authenticated_get(&format!("/api/v1/conversations/{newer}"), &outsider_cookie),
        )
        .await;
        assert_eq!(hidden.status(), StatusCode::NOT_FOUND);
        assert_eq!(hidden.headers()[CONTENT_TYPE], "application/problem+json");
        store.close().await;
    }

    #[tokio::test]
    async fn chat_routes_apply_stable_authentication_and_validation_errors() {
        let (app, store, _directory) = test_app().await;
        let user = Chat::new(store.clone())
            .create_user(CreateUser::new("Alice"))
            .await
            .expect("user can be created")
            .user()
            .id();
        let cookie = session_cookie(&store, user).await;

        let unauthenticated = request(
            app.clone(),
            Request::builder()
                .uri("/api/v1/conversations")
                .body(Body::empty())
                .expect("test request is valid"),
        )
        .await;
        assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(unauthenticated.headers()[CACHE_CONTROL], "no-store");
        assert_eq!(
            unauthenticated.headers()[WWW_AUTHENTICATE],
            "ChatSession realm=\"chat\""
        );

        let malformed = request(
            app.clone(),
            authenticated_get("/api/v1/conversations/not-an-id", &cookie),
        )
        .await;
        assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);

        let unknown = request(
            app.clone(),
            authenticated_get("/api/v1/conversations?offset=1", &cookie),
        )
        .await;
        assert_eq!(unknown.status(), StatusCode::BAD_REQUEST);

        let invalid_limit = request(
            app,
            authenticated_get("/api/v1/conversations?limit=0", &cookie),
        )
        .await;
        assert_eq!(invalid_limit.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let problem = response_json(invalid_limit).await;
        assert_eq!(problem["errors"][0]["field"], "limit");
        assert_eq!(problem["errors"][0]["max"], MAX_CONVERSATION_PAGE_SIZE);
        store.close().await;
    }

    #[tokio::test]
    async fn authenticated_mutations_create_retrievable_resources() {
        let (app, store, _directory) = test_app().await;
        let chat = Chat::new(store.clone());
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
        let (owner_cookie, owner_csrf) = session_credentials(&store, owner).await;
        let (member_cookie, member_csrf) = session_credentials(&store, member).await;
        let (outsider_cookie, outsider_csrf) = session_credentials(&store, outsider).await;

        let response = request(
            app.clone(),
            authenticated_json_post(
                "/api/v1/conversations",
                &owner_cookie,
                &owner_csrf,
                r#"{"title":"General"}"#,
            ),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
        let location = response.headers()[LOCATION]
            .to_str()
            .expect("location is valid")
            .to_owned();
        let conversation = response_json(response).await;
        let conversation_id = conversation["id"]
            .as_str()
            .expect("conversation ID is a string")
            .parse::<i64>()
            .ok()
            .and_then(|value| chat::ConversationId::new(value).ok())
            .expect("conversation ID is valid");
        assert_eq!(location, format!("/api/v1/conversations/{conversation_id}"));
        assert_eq!(conversation["title"], "General");
        assert_eq!(conversation["role"], "owner");

        chat.add_member(owner, AddMember::new(conversation_id, member))
            .await
            .expect("member can be added");
        let response = request(
            app.clone(),
            authenticated_json_post(
                &format!("/api/v1/conversations/{conversation_id}/messages"),
                &member_cookie,
                &member_csrf,
                r#"{"body":"hello"}"#,
            ),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
        let message_location = response.headers()[LOCATION]
            .to_str()
            .expect("location is valid")
            .to_owned();
        let created = response_json(response).await;
        assert_eq!(created["conversation_id"], conversation_id.to_string());
        assert_eq!(created["author_id"], member.to_string());
        assert_eq!(created["body"], "hello");
        assert_eq!(
            message_location,
            format!(
                "/api/v1/conversations/{conversation_id}/messages/{}",
                created["id"].as_str().expect("message ID is a string")
            )
        );

        let fetched = request(
            app.clone(),
            authenticated_get(&message_location, &owner_cookie),
        )
        .await;
        assert_eq!(fetched.status(), StatusCode::OK);
        assert_eq!(response_json(fetched).await, created);

        let hidden_read = request(
            app.clone(),
            authenticated_get(&message_location, &outsider_cookie),
        )
        .await;
        assert_eq!(hidden_read.status(), StatusCode::NOT_FOUND);

        let hidden_write = request(
            app,
            authenticated_json_post(
                &format!("/api/v1/conversations/{conversation_id}/messages"),
                &outsider_cookie,
                &outsider_csrf,
                r#"{"body":"denied"}"#,
            ),
        )
        .await;
        assert_eq!(hidden_write.status(), StatusCode::NOT_FOUND);
        store.close().await;
    }

    #[tokio::test]
    async fn mutation_prerequisites_are_checked_before_json() {
        let (app, store, _directory) = test_app().await;
        let chat = Chat::new(store.clone());
        let user = chat
            .create_user(CreateUser::new("Alice"))
            .await
            .expect("user can be created")
            .user()
            .id();
        let (cookie, csrf) = session_credentials(&store, user).await;

        let unauthenticated = request(
            app.clone(),
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/conversations")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from("{"))
                .expect("test request is valid"),
        )
        .await;
        assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

        let missing_csrf = request(
            app.clone(),
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/conversations")
                .header(CONTENT_TYPE, "application/json")
                .header(COOKIE, &cookie)
                .header(ORIGIN, "http://127.0.0.1:3000")
                .body(Body::from("{"))
                .expect("test request is valid"),
        )
        .await;
        assert_eq!(missing_csrf.status(), StatusCode::FORBIDDEN);

        let wrong_origin = request(
            app.clone(),
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/conversations")
                .header(CONTENT_TYPE, "application/json")
                .header(COOKIE, &cookie)
                .header(ORIGIN, "https://attacker.example")
                .header("x-csrf-token", &csrf)
                .body(Body::from(r#"{"title":"Denied"}"#))
                .expect("test request is valid"),
        )
        .await;
        assert_eq!(wrong_origin.status(), StatusCode::FORBIDDEN);

        let wrong_csrf = request(
            app.clone(),
            authenticated_json_post(
                "/api/v1/conversations",
                &cookie,
                "invalid-token",
                r#"{"title":"Denied"}"#,
            ),
        )
        .await;
        assert_eq!(wrong_csrf.status(), StatusCode::FORBIDDEN);

        let conversations = chat
            .list_conversations(user, ListConversations::new())
            .await
            .expect("conversations can be listed");
        assert!(conversations.conversations().is_empty());
        store.close().await;
    }

    #[tokio::test]
    async fn mutation_json_and_domain_errors_are_finite() {
        let (app, store, _directory) = test_app().await;
        let chat = Chat::new(store.clone());
        let user = chat
            .create_user(CreateUser::new("Alice"))
            .await
            .expect("user can be created")
            .user()
            .id();
        let conversation = chat
            .create_conversation(user, CreateConversation::new("General"))
            .await
            .expect("conversation can be created")
            .conversation()
            .id();
        let (cookie, csrf) = session_credentials(&store, user).await;

        let unsupported = request(
            app.clone(),
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/conversations")
                .header(COOKIE, &cookie)
                .header(ORIGIN, "http://127.0.0.1:3000")
                .header("x-csrf-token", &csrf)
                .body(Body::from(r#"{"title":"No media type"}"#))
                .expect("test request is valid"),
        )
        .await;
        assert_problem(
            unsupported,
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported-media-type",
        )
        .await;

        for body in [
            "{",
            "{}",
            r#"{"title":null}"#,
            r#"{"title":1}"#,
            r#"{"title":"General","unknown":true}"#,
        ] {
            let invalid = request(
                app.clone(),
                authenticated_json_post("/api/v1/conversations", &cookie, &csrf, body),
            )
            .await;
            assert_problem(invalid, StatusCode::BAD_REQUEST, "invalid-request").await;
        }

        let oversized_body = serde_json::json!({ "body": "x".repeat(70 * 1024) }).to_string();
        let oversized = request(
            app.clone(),
            authenticated_json_post(
                &format!("/api/v1/conversations/{conversation}/messages"),
                &cookie,
                &csrf,
                oversized_body,
            ),
        )
        .await;
        assert_problem(
            oversized,
            StatusCode::PAYLOAD_TOO_LARGE,
            "content-too-large",
        )
        .await;

        for (body, field, code, max) in [
            (String::from(r#"{"title":"   "}"#), "title", "empty", None),
            (
                String::from(r#"{"title":"bad\nname"}"#),
                "title",
                "contains_control_character",
                None,
            ),
            (
                serde_json::json!({
                    "title": "x".repeat(MAX_CONVERSATION_TITLE_CHARS + 1)
                })
                .to_string(),
                "title",
                "too_long",
                Some(MAX_CONVERSATION_TITLE_CHARS),
            ),
        ] {
            let response = request(
                app.clone(),
                authenticated_json_post("/api/v1/conversations", &cookie, &csrf, body),
            )
            .await;
            assert_validation(response, field, code, max).await;
        }

        for (body, code, max) in [
            (String::from(r#"{"body":"   "}"#), "empty", None),
            (
                serde_json::json!({ "body": "x".repeat(MAX_MESSAGE_BODY_CHARS + 1) }).to_string(),
                "too_long",
                Some(MAX_MESSAGE_BODY_CHARS),
            ),
        ] {
            let response = request(
                app.clone(),
                authenticated_json_post(
                    &format!("/api/v1/conversations/{conversation}/messages"),
                    &cookie,
                    &csrf,
                    body,
                ),
            )
            .await;
            assert_validation(response, "body", code, max).await;
        }

        let conversations = chat
            .list_conversations(user, ListConversations::new())
            .await
            .expect("conversations can be listed");
        assert_eq!(conversations.conversations().len(), 1);
        let messages = chat
            .list_messages(user, ListMessages::new(conversation))
            .await
            .expect("messages can be listed");
        assert!(messages.messages().is_empty());
        store.close().await;
    }

    async fn assert_problem(response: Response, status: StatusCode, suffix: &str) {
        assert_eq!(response.status(), status);
        assert_eq!(response.headers()[CONTENT_TYPE], "application/problem+json");
        assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
        let problem = response_json(response).await;
        assert_eq!(problem["status"], status.as_u16());
        assert_eq!(problem["type"], format!("urn:chat-rs:problem:{suffix}"));
    }

    async fn assert_validation(response: Response, field: &str, code: &str, max: Option<usize>) {
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let problem = response_json(response).await;
        assert_eq!(problem["errors"][0]["field"], field);
        assert_eq!(problem["errors"][0]["code"], code);
        match max {
            Some(max) => assert_eq!(problem["errors"][0]["max"], max),
            None => assert!(problem["errors"][0].get("max").is_none()),
        }
    }
}
