mod connection;
mod hub;
mod protocol;
#[cfg(test)]
mod tests;

use std::time::Duration;

pub(crate) use connection::run_connection;
pub(crate) use hub::{
    CapacityError, CloseDirective, ConnectionRegistration, RealtimeHub, SubscribeResult,
};
pub(crate) use protocol::{
    ClientMessage, ServerMessage, decode_client_message, encode_server_message,
};

pub(crate) const SUBPROTOCOL: &str = "chat.v1";

#[derive(Clone, Copy, Debug)]
pub(crate) struct RealtimeSettings {
    pub(crate) max_connections: usize,
    pub(crate) max_connections_per_user: usize,
    pub(crate) max_subscriptions_per_connection: usize,
    pub(crate) outbound_queue_capacity: usize,
    pub(crate) max_message_size: usize,
    pub(crate) max_frame_size: usize,
    pub(crate) read_buffer_size: usize,
    pub(crate) write_buffer_size: usize,
    pub(crate) max_write_buffer_size: usize,
    pub(crate) heartbeat_interval: Duration,
    pub(crate) heartbeat_timeout: Duration,
    pub(crate) close_handshake_timeout: Duration,
    pub(crate) server_drain_timeout: Duration,
}

impl Default for RealtimeSettings {
    fn default() -> Self {
        Self {
            max_connections: 1024,
            max_connections_per_user: 8,
            max_subscriptions_per_connection: 128,
            outbound_queue_capacity: 64,
            max_message_size: 4 * 1024,
            max_frame_size: 4 * 1024,
            read_buffer_size: 16 * 1024,
            write_buffer_size: 16 * 1024,
            max_write_buffer_size: 64 * 1024,
            heartbeat_interval: Duration::from_secs(30),
            heartbeat_timeout: Duration::from_secs(90),
            close_handshake_timeout: Duration::from_secs(2),
            server_drain_timeout: Duration::from_secs(10),
        }
    }
}
