use std::time::SystemTime;

use axum::extract::ws::{CloseFrame, Message, WebSocket};
use chat::{Chat, GetConversationError};
use tokio::time::{self, Instant, MissedTickBehavior};

use crate::sqlite::SqliteStore;

use super::{
    ClientMessage, CloseDirective, ConnectionRegistration, RealtimeHub, RealtimeSettings,
    ServerMessage, SubscribeResult, decode_client_message, encode_server_message,
    protocol::SubscriptionRejection,
};

pub(crate) async fn run_connection(
    mut socket: WebSocket,
    mut registration: ConnectionRegistration,
    hub: RealtimeHub,
    chat: Chat<SqliteStore>,
) {
    let settings = hub.settings();
    let connection_id = registration.connection_id();
    let user_id = registration.user_id();
    let expires_at = registration.expires_at();
    let Some((mut outbound, mut close)) = registration.take_channels() else {
        tracing::error!(%connection_id, %user_id, "realtime connection channels are unavailable");
        return;
    };
    let connected_at = Instant::now();

    tracing::info!(%connection_id, %user_id, "realtime connection established");

    let initial_close = {
        let borrowed = close.borrow();
        *borrowed
    };
    if let Some(directive) = initial_close {
        close_socket(&mut socket, directive, settings).await;
        return;
    }
    if send_server_message(&mut socket, &ServerMessage::Ready)
        .await
        .is_err()
    {
        tracing::debug!(%connection_id, %user_id, "realtime connection closed before ready");
        return;
    }

    let mut heartbeat = time::interval_at(
        Instant::now() + settings.heartbeat_interval,
        settings.heartbeat_interval,
    );
    heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let expiry_deadline = system_time_deadline(expires_at);
    let expiry = time::sleep_until(expiry_deadline);
    tokio::pin!(expiry);
    let mut ping_sequence = 0_u64;
    let mut pending_ping: Option<[u8; 8]> = None;
    let mut pong_deadline: Option<Instant> = None;
    let mut close_category = "peer_closed";

    loop {
        let pong_timeout = time::sleep_until(
            pong_deadline.unwrap_or_else(|| Instant::now() + settings.heartbeat_timeout),
        );
        tokio::pin!(pong_timeout);

        tokio::select! {
            biased;

            changed = close.changed() => {
                if changed.is_err() {
                    close_category = "hub_closed";
                    break;
                }
                let directive = {
                    let borrowed = close.borrow_and_update();
                    *borrowed
                };
                if let Some(directive) = directive {
                    close_category = directive.category;
                    close_socket(&mut socket, directive, settings).await;
                    break;
                }
            }

            _ = &mut expiry => {
                let directive = CloseDirective::session_revoked();
                close_category = "session_expired";
                close_socket(&mut socket, directive, settings).await;
                break;
            }

            _ = &mut pong_timeout, if pending_ping.is_some() => {
                let directive = CloseDirective::heartbeat_timeout();
                close_category = directive.category;
                close_socket(&mut socket, directive, settings).await;
                break;
            }

            message = outbound.recv() => {
                let Some(message) = message else {
                    close_category = "outbound_closed";
                    break;
                };
                if socket.send(Message::Text(message.as_ref().into())).await.is_err() {
                    close_category = "write_failed";
                    break;
                }
            }

            incoming = socket.recv() => {
                let Some(incoming) = incoming else {
                    break;
                };
                let message = match incoming {
                    Ok(message) => message,
                    Err(error) => {
                        tracing::debug!(%connection_id, %user_id, error = %error, "realtime read failed");
                        close_category = "read_failed";
                        break;
                    }
                };
                match message {
                    Message::Text(text) => {
                        match handle_client_message(
                            &mut socket,
                            text.as_str(),
                            connection_id,
                            user_id,
                            &hub,
                            &chat,
                        )
                        .await
                        {
                            ControlOutcome::Continue => {}
                            ControlOutcome::Stop(directive) => {
                                close_category = directive.category;
                                close_socket(&mut socket, directive, settings).await;
                                break;
                            }
                            ControlOutcome::Disconnected => {
                                close_category = "write_failed";
                                break;
                            }
                        }
                    }
                    Message::Binary(_) => {
                        let directive = CloseDirective::unsupported_data();
                        close_category = directive.category;
                        close_socket(&mut socket, directive, settings).await;
                        break;
                    }
                    Message::Ping(_) => {}
                    Message::Pong(payload) => {
                        if pending_ping
                            .as_ref()
                            .is_some_and(|expected| payload.as_ref() == expected.as_slice())
                        {
                            pending_ping = None;
                            pong_deadline = None;
                        }
                    }
                    Message::Close(_) => break,
                }
            }

            _ = heartbeat.tick() => {
                if pending_ping.is_none() {
                    ping_sequence = ping_sequence.wrapping_add(1);
                    let payload = ping_sequence.to_be_bytes();
                    if socket.send(Message::Ping(payload.to_vec().into())).await.is_err() {
                        close_category = "heartbeat_write_failed";
                        break;
                    }
                    pending_ping = Some(payload);
                    pong_deadline = Some(Instant::now() + settings.heartbeat_timeout);
                }
            }
        }
    }

    let duration_ms = u64::try_from(connected_at.elapsed().as_millis()).unwrap_or(u64::MAX);
    tracing::info!(
        %connection_id,
        %user_id,
        close_category,
        duration_ms,
        "realtime connection ended"
    );
}

enum ControlOutcome {
    Continue,
    Stop(CloseDirective),
    Disconnected,
}

async fn handle_client_message(
    socket: &mut WebSocket,
    value: &str,
    connection_id: super::hub::ConnectionId,
    user_id: chat::UserId,
    hub: &RealtimeHub,
    chat: &Chat<SqliteStore>,
) -> ControlOutcome {
    let message = match decode_client_message(value) {
        Ok(message) => message,
        Err(_) => return ControlOutcome::Stop(CloseDirective::policy_violation()),
    };
    let conversation_id = match message.conversation_id() {
        Ok(conversation_id) => conversation_id,
        Err(_) => {
            let response = ServerMessage::subscription_rejected(
                message.raw_conversation_id(),
                SubscriptionRejection::InvalidRequest,
            );
            return send_control_response(socket, &response).await;
        }
    };

    match message {
        ClientMessage::Subscribe { .. } => {
            match chat.get_conversation(user_id, conversation_id).await {
                Ok(_) => match hub.subscribe(connection_id, conversation_id) {
                    SubscribeResult::Added | SubscribeResult::AlreadySubscribed => {
                        send_control_response(socket, &ServerMessage::subscribed(conversation_id))
                            .await
                    }
                    SubscribeResult::LimitReached => {
                        send_control_response(
                            socket,
                            &ServerMessage::subscription_rejected(
                                conversation_id.to_string(),
                                SubscriptionRejection::LimitReached,
                            ),
                        )
                        .await
                    }
                    SubscribeResult::ConnectionClosed => ControlOutcome::Disconnected,
                },
                Err(GetConversationError::NotFound) => {
                    send_control_response(
                        socket,
                        &ServerMessage::subscription_rejected(
                            conversation_id.to_string(),
                            SubscriptionRejection::NotFound,
                        ),
                    )
                    .await
                }
                Err(GetConversationError::StoreUnavailable) => {
                    send_control_response(
                        socket,
                        &ServerMessage::subscription_rejected(
                            conversation_id.to_string(),
                            SubscriptionRejection::TemporarilyUnavailable,
                        ),
                    )
                    .await
                }
                Err(GetConversationError::InvalidStoreResult) => {
                    tracing::error!(%connection_id, "conversation store returned invalid realtime subscription state");
                    ControlOutcome::Stop(CloseDirective::internal_error())
                }
            }
        }
        ClientMessage::Unsubscribe { .. } => {
            if hub.unsubscribe(connection_id, conversation_id) {
                send_control_response(socket, &ServerMessage::unsubscribed(conversation_id)).await
            } else {
                ControlOutcome::Disconnected
            }
        }
    }
}

async fn send_control_response(socket: &mut WebSocket, message: &ServerMessage) -> ControlOutcome {
    match send_server_message(socket, message).await {
        Ok(()) => ControlOutcome::Continue,
        Err(SendError::Serialization) => ControlOutcome::Stop(CloseDirective::internal_error()),
        Err(SendError::Socket) => ControlOutcome::Disconnected,
    }
}

enum SendError {
    Serialization,
    Socket,
}

async fn send_server_message(
    socket: &mut WebSocket,
    message: &ServerMessage,
) -> Result<(), SendError> {
    let message = encode_server_message(message).map_err(|error| {
        tracing::error!(error = %error, "realtime response serialization failed");
        SendError::Serialization
    })?;
    socket
        .send(Message::Text(message.as_ref().into()))
        .await
        .map_err(|_| SendError::Socket)
}

async fn close_socket(
    socket: &mut WebSocket,
    directive: CloseDirective,
    settings: RealtimeSettings,
) {
    let frame = CloseFrame {
        code: directive.code,
        reason: directive.reason.into(),
    };
    if socket.send(Message::Close(Some(frame))).await.is_err() {
        return;
    }
    let _ = time::timeout(settings.close_handshake_timeout, async {
        while let Some(message) = socket.recv().await {
            match message {
                Ok(Message::Close(_)) | Err(_) => break,
                Ok(_) => {}
            }
        }
    })
    .await;
}

fn system_time_deadline(deadline: SystemTime) -> Instant {
    let remaining = deadline
        .duration_since(SystemTime::now())
        .unwrap_or_default();
    Instant::now() + remaining
}
