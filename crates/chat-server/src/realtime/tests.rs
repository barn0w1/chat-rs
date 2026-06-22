use std::time::{Duration, SystemTime};

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header::CONTENT_TYPE},
};
use chat::{AddMember, Chat, CreateConversation, CreateUser};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::{net::TcpListener, time};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{
        Error as WebSocketError, Message,
        client::IntoClientRequest,
        http::{
            HeaderValue,
            header::{COOKIE, ORIGIN, SEC_WEBSOCKET_PROTOCOL},
        },
    },
};
use tower::ServiceExt;

use crate::{Config, app, auth::AuthStore, config::ConfigValues, sqlite::SqliteStore};

use super::{RealtimeHub, RealtimeSettings, SUBPROTOCOL};

type ClientSocket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

#[tokio::test]
async fn two_subscribers_receive_committed_message_notification_and_fetch_state() {
    let directory = tempfile::tempdir().expect("temporary directory can be created");
    let store = SqliteStore::open(directory.path().join("chat.sqlite3"))
        .await
        .expect("test database can be opened");
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
    let conversation_id = chat
        .create_conversation(owner, CreateConversation::new("Realtime"))
        .await
        .expect("conversation can be created")
        .conversation()
        .id();
    chat.add_member(owner, AddMember::new(conversation_id, member))
        .await
        .expect("member can be added");

    let auth = AuthStore::new(store.clone());
    let now = SystemTime::now();
    let owner_session = auth.issue_session(owner, None, now).await.unwrap();
    let member_session = auth.issue_session(member, None, now).await.unwrap();
    let outsider_session = auth.issue_session(outsider, None, now).await.unwrap();
    let owner_token = owner_session.session_token();
    let member_token = member_session.session_token();
    let outsider_token = outsider_session.session_token();
    let member_csrf = auth
        .resolve_session(&member_token, now)
        .await
        .unwrap()
        .unwrap()
        .csrf_token();

    let config = Config::from_values(ConfigValues {
        database_path: Some(directory.path().join("chat.sqlite3").into_os_string()),
        ..ConfigValues::default()
    })
    .unwrap();
    let hub = RealtimeHub::new(RealtimeSettings {
        heartbeat_interval: Duration::from_secs(5),
        heartbeat_timeout: Duration::from_secs(10),
        ..RealtimeSettings::default()
    });
    let router = app::router(store.clone(), &config, None, hub);
    let http_router = router.clone();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    let mut owner_socket = connect(address, &owner_token).await;
    let mut member_socket = connect(address, &member_token).await;
    let mut outsider_socket = connect(address, &outsider_token).await;
    assert_eq!(next_json(&mut owner_socket).await["type"], "ready");
    assert_eq!(next_json(&mut member_socket).await["type"], "ready");
    assert_eq!(next_json(&mut outsider_socket).await["type"], "ready");
    subscribe(&mut owner_socket, conversation_id.to_string()).await;
    subscribe(&mut owner_socket, conversation_id.to_string()).await;
    subscribe(&mut member_socket, conversation_id.to_string()).await;
    outsider_socket
        .send(Message::Text(
            format!(r#"{{"type":"subscribe","conversation_id":"{conversation_id}"}}"#).into(),
        ))
        .await
        .unwrap();
    let rejected = next_json(&mut outsider_socket).await;
    assert_eq!(rejected["type"], "subscription_rejected");
    assert_eq!(rejected["reason"], "not_found");

    let response = http_router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/conversations/{conversation_id}/messages"))
                .header(CONTENT_TYPE, "application/json")
                .header(COOKIE, format!("chat_session={member_token}"))
                .header(ORIGIN, "http://127.0.0.1:3000")
                .header("x-csrf-token", &member_csrf)
                .body(Body::from(r#"{"body":"hello realtime"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let owner_event = next_json(&mut owner_socket).await;
    let member_event = next_json(&mut member_socket).await;
    assert_eq!(owner_event["type"], "message_posted");
    assert_eq!(member_event, owner_event);
    assert_eq!(owner_event["conversation_id"], conversation_id.to_string());
    assert!(
        time::timeout(Duration::from_millis(100), owner_socket.next())
            .await
            .is_err(),
        "duplicate subscription must not duplicate notifications"
    );
    assert!(
        time::timeout(Duration::from_millis(100), outsider_socket.next())
            .await
            .is_err(),
        "an unauthorized connection must not receive conversation notifications"
    );
    let message_id = owner_event["message_id"]
        .as_str()
        .expect("notification has a message ID");

    let fetched = http_router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/conversations/{conversation_id}/messages/{message_id}"
                ))
                .header(COOKIE, format!("chat_session={member_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(fetched.status(), StatusCode::OK);
    let fetched = to_bytes(fetched.into_body(), 4096).await.unwrap();
    let fetched: Value = serde_json::from_slice(&fetched).unwrap();
    assert_eq!(fetched["body"], "hello realtime");

    let logout = http_router
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/session")
                .header(COOKIE, format!("chat_session={member_token}"))
                .header(ORIGIN, "http://127.0.0.1:3000")
                .header("x-csrf-token", &member_csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(logout.status(), StatusCode::NO_CONTENT);
    let closed = time::timeout(Duration::from_secs(2), member_socket.next())
        .await
        .expect("session close timed out")
        .expect("server sends a close frame")
        .expect("close frame is valid");
    assert!(matches!(
        closed,
        Message::Close(Some(frame)) if u16::from(frame.code) == 1008
    ));

    owner_socket.close(None).await.unwrap();
    outsider_socket.close(None).await.unwrap();
    server.abort();
    let _ = server.await;
    store.close().await;
}

#[tokio::test]
async fn graceful_shutdown_sends_restart_close_and_drains_the_connection() {
    let directory = tempfile::tempdir().expect("temporary directory can be created");
    let store = SqliteStore::open(directory.path().join("chat.sqlite3"))
        .await
        .expect("test database can be opened");
    let user_id = Chat::new(store.clone())
        .create_user(CreateUser::new("Alice"))
        .await
        .unwrap()
        .user()
        .id();
    let token = AuthStore::new(store.clone())
        .issue_session(user_id, None, SystemTime::now())
        .await
        .unwrap()
        .session_token();
    let config = Config::from_values(ConfigValues {
        database_path: Some(directory.path().join("chat.sqlite3").into_os_string()),
        ..ConfigValues::default()
    })
    .unwrap();
    let hub = RealtimeHub::new(RealtimeSettings::default());
    let router = app::router(store.clone(), &config, None, hub.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });

    let mut socket = connect(address, &token).await;
    assert_eq!(next_json(&mut socket).await["type"], "ready");
    hub.shutdown();
    let _ = shutdown_tx.send(());

    let closed = time::timeout(Duration::from_secs(2), socket.next())
        .await
        .expect("shutdown close timed out")
        .expect("server sends a close frame")
        .expect("close frame is valid");
    assert!(matches!(
        closed,
        Message::Close(Some(frame)) if u16::from(frame.code) == 1012
    ));
    drop(socket);
    time::timeout(Duration::from_secs(2), server)
        .await
        .expect("server shutdown timed out")
        .expect("server task completed");
    hub.wait_for_drain().await;
    store.close().await;
}

#[tokio::test]
async fn heartbeat_closes_a_client_that_does_not_process_ping() {
    let directory = tempfile::tempdir().expect("temporary directory can be created");
    let store = SqliteStore::open(directory.path().join("chat.sqlite3"))
        .await
        .expect("test database can be opened");
    let user_id = Chat::new(store.clone())
        .create_user(CreateUser::new("Alice"))
        .await
        .unwrap()
        .user()
        .id();
    let token = AuthStore::new(store.clone())
        .issue_session(user_id, None, SystemTime::now())
        .await
        .unwrap()
        .session_token();
    let config = Config::from_values(ConfigValues {
        database_path: Some(directory.path().join("chat.sqlite3").into_os_string()),
        ..ConfigValues::default()
    })
    .unwrap();
    let hub = RealtimeHub::new(RealtimeSettings {
        heartbeat_interval: Duration::from_millis(20),
        heartbeat_timeout: Duration::from_millis(40),
        close_handshake_timeout: Duration::from_millis(20),
        ..RealtimeSettings::default()
    });
    let router = app::router(store.clone(), &config, None, hub);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    let mut socket = connect(address, &token).await;
    assert_eq!(next_json(&mut socket).await["type"], "ready");
    time::sleep(Duration::from_millis(100)).await;

    let closed = time::timeout(Duration::from_secs(1), async {
        while let Some(message) = socket.next().await {
            if let Message::Close(frame) = message.unwrap() {
                return frame;
            }
        }
        None
    })
    .await
    .expect("heartbeat close timed out")
    .expect("server sends a close frame");
    assert_eq!(u16::from(closed.code), 1001);

    server.abort();
    let _ = server.await;
    store.close().await;
}

#[tokio::test]
async fn opening_handshake_rejects_missing_auth_wrong_origin_and_protocol() {
    let directory = tempfile::tempdir().expect("temporary directory can be created");
    let store = SqliteStore::open(directory.path().join("chat.sqlite3"))
        .await
        .expect("test database can be opened");
    let user_id = Chat::new(store.clone())
        .create_user(CreateUser::new("Alice"))
        .await
        .unwrap()
        .user()
        .id();
    let token = AuthStore::new(store.clone())
        .issue_session(user_id, None, SystemTime::now())
        .await
        .unwrap()
        .session_token();
    let config = Config::from_values(ConfigValues {
        database_path: Some(directory.path().join("chat.sqlite3").into_os_string()),
        ..ConfigValues::default()
    })
    .unwrap();
    let router = app::router(
        store.clone(),
        &config,
        None,
        RealtimeHub::new(RealtimeSettings {
            max_connections: 0,
            ..RealtimeSettings::default()
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    assert_handshake_status(
        websocket_request(
            address,
            None,
            Some("http://127.0.0.1:3000"),
            Some(SUBPROTOCOL),
        ),
        StatusCode::UNAUTHORIZED,
    )
    .await;
    assert_handshake_status(
        websocket_request(
            address,
            Some(&token),
            Some("https://invalid.example"),
            Some(SUBPROTOCOL),
        ),
        StatusCode::FORBIDDEN,
    )
    .await;
    assert_handshake_status(
        websocket_request(address, Some(&token), Some("http://127.0.0.1:3000"), None),
        StatusCode::BAD_REQUEST,
    )
    .await;
    assert_handshake_status(
        websocket_request(address, Some(&token), None, Some(SUBPROTOCOL)),
        StatusCode::FORBIDDEN,
    )
    .await;
    assert_handshake_status(
        websocket_request(
            address,
            Some(&token),
            Some("http://127.0.0.1:3000"),
            Some("unsupported.v1"),
        ),
        StatusCode::BAD_REQUEST,
    )
    .await;
    assert_handshake_status(
        websocket_request(
            address,
            Some(&token),
            Some("http://127.0.0.1:3000"),
            Some(SUBPROTOCOL),
        ),
        StatusCode::SERVICE_UNAVAILABLE,
    )
    .await;

    server.abort();
    let _ = server.await;
    store.close().await;
}

#[tokio::test]
async fn unsupported_and_malformed_application_messages_close_the_connection() {
    let directory = tempfile::tempdir().expect("temporary directory can be created");
    let store = SqliteStore::open(directory.path().join("chat.sqlite3"))
        .await
        .expect("test database can be opened");
    let user_id = Chat::new(store.clone())
        .create_user(CreateUser::new("Alice"))
        .await
        .unwrap()
        .user()
        .id();
    let token = AuthStore::new(store.clone())
        .issue_session(user_id, None, SystemTime::now())
        .await
        .unwrap()
        .session_token();
    let config = Config::from_values(ConfigValues {
        database_path: Some(directory.path().join("chat.sqlite3").into_os_string()),
        ..ConfigValues::default()
    })
    .unwrap();
    let router = app::router(
        store.clone(),
        &config,
        None,
        RealtimeHub::new(RealtimeSettings::default()),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    let mut binary = connect(address, &token).await;
    let _ = next_json(&mut binary).await;
    binary
        .send(Message::Binary(vec![1, 2, 3].into()))
        .await
        .unwrap();
    assert_close_code(&mut binary, 1003).await;

    let mut malformed = connect(address, &token).await;
    let _ = next_json(&mut malformed).await;
    malformed
        .send(Message::Text(r#"{"type":"unknown"}"#.into()))
        .await
        .unwrap();
    assert_close_code(&mut malformed, 1008).await;

    server.abort();
    let _ = server.await;
    store.close().await;
}

async fn connect(address: std::net::SocketAddr, token: &str) -> ClientSocket {
    let request = websocket_request(
        address,
        Some(token),
        Some("http://127.0.0.1:3000"),
        Some(SUBPROTOCOL),
    );
    let (socket, response) = connect_async(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
    assert_eq!(response.headers()[SEC_WEBSOCKET_PROTOCOL], SUBPROTOCOL);
    socket
}

fn websocket_request(
    address: std::net::SocketAddr,
    token: Option<&str>,
    origin: Option<&str>,
    protocol: Option<&str>,
) -> tokio_tungstenite::tungstenite::http::Request<()> {
    let mut request = format!("ws://{address}/api/v1/ws")
        .into_client_request()
        .unwrap();
    if let Some(origin) = origin {
        request
            .headers_mut()
            .insert(ORIGIN, HeaderValue::from_str(origin).unwrap());
    }
    if let Some(token) = token {
        request.headers_mut().insert(
            COOKIE,
            HeaderValue::from_str(&format!("chat_session={token}")).unwrap(),
        );
    }
    if let Some(protocol) = protocol {
        request.headers_mut().insert(
            SEC_WEBSOCKET_PROTOCOL,
            HeaderValue::from_str(protocol).unwrap(),
        );
    }
    request
}

async fn assert_handshake_status(
    request: tokio_tungstenite::tungstenite::http::Request<()>,
    expected: StatusCode,
) {
    let error = match connect_async(request).await {
        Ok(_) => panic!("handshake should be rejected"),
        Err(error) => error,
    };
    let WebSocketError::Http(response) = error else {
        panic!("expected HTTP handshake rejection, got {error}");
    };
    assert_eq!(response.status(), expected);
}

async fn assert_close_code(socket: &mut ClientSocket, expected: u16) {
    let message = time::timeout(Duration::from_secs(2), socket.next())
        .await
        .expect("close timed out")
        .expect("server sends a close frame")
        .expect("close frame is valid");
    assert!(matches!(
        message,
        Message::Close(Some(frame)) if u16::from(frame.code) == expected
    ));
}

async fn subscribe(socket: &mut ClientSocket, conversation_id: String) {
    socket
        .send(Message::Text(
            format!(r#"{{"type":"subscribe","conversation_id":"{conversation_id}"}}"#).into(),
        ))
        .await
        .unwrap();
    let response = next_json(socket).await;
    assert_eq!(response["type"], "subscribed");
    assert_eq!(response["conversation_id"], conversation_id);
}

async fn next_json(socket: &mut ClientSocket) -> Value {
    let message = time::timeout(Duration::from_secs(2), socket.next())
        .await
        .expect("realtime response timed out")
        .expect("realtime connection ended")
        .expect("realtime response is valid");
    let Message::Text(text) = message else {
        panic!("expected text message, got {message:?}");
    };
    serde_json::from_str(text.as_str()).expect("realtime response is JSON")
}
