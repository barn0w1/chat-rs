use std::time::SystemTime;

use axum::{
    Router,
    extract::{State, WebSocketUpgrade},
    response::{IntoResponse, Response},
    routing::any,
};

use crate::{
    app::AppState,
    realtime::{CapacityError, SUBPROTOCOL, run_connection},
};

use super::{authentication::AuthenticatedWebSocket, problem::Problem};

pub(super) fn routes() -> Router<AppState> {
    Router::new().route("/api/v1/ws", any(upgrade))
}

async fn upgrade(
    State(state): State<AppState>,
    authenticated: AuthenticatedWebSocket,
    websocket: WebSocketUpgrade,
) -> Response {
    let websocket = websocket.protocols([SUBPROTOCOL]);
    if websocket.selected_protocol().is_none() {
        return Problem::invalid_request().into_response();
    }

    let session = authenticated.into_session();
    let registration = match state.realtime.reserve(
        session.user_id(),
        session.fingerprint(),
        session.expires_at(),
    ) {
        Ok(registration) => registration,
        Err(
            CapacityError::ShuttingDown | CapacityError::GlobalLimit | CapacityError::UserLimit,
        ) => return Problem::unavailable().into_response(),
    };

    match state
        .auth
        .session_is_active(registration.session(), SystemTime::now())
        .await
    {
        Ok(true) => {}
        Ok(false) => return Problem::unauthenticated().into_response(),
        Err(error) => {
            tracing::error!(error = %error, "realtime session revalidation failed");
            return super::authentication::auth_operation_problem(&error).into_response();
        }
    }

    let settings = state.realtime.settings();
    let connection_id = registration.connection_id();
    let user_id = registration.user_id();
    let hub = state.realtime.clone();
    let chat = state.chat.clone();
    websocket
        .read_buffer_size(settings.read_buffer_size)
        .write_buffer_size(settings.write_buffer_size)
        .max_write_buffer_size(settings.max_write_buffer_size)
        .max_message_size(settings.max_message_size)
        .max_frame_size(settings.max_frame_size)
        .accept_unmasked_frames(false)
        .on_failed_upgrade(move |error| {
            tracing::debug!(%connection_id, %user_id, error = %error, "realtime upgrade failed");
        })
        .on_upgrade(move |socket| run_connection(socket, registration, hub, chat))
}
