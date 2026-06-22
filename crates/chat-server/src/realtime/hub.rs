use std::{
    collections::{HashMap, HashSet},
    fmt,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicUsize, Ordering},
    },
    time::SystemTime,
};

use chat::{ChatEvent, ConversationId, UserId};
use tokio::sync::{Notify, mpsc, watch};

use crate::auth::SessionFingerprint;

use super::{RealtimeSettings, ServerMessage, encode_server_message};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ConnectionId(u64);

impl fmt::Display for ConnectionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CloseDirective {
    pub(crate) code: u16,
    pub(crate) reason: &'static str,
    pub(crate) category: &'static str,
}

impl CloseDirective {
    pub(crate) const fn heartbeat_timeout() -> Self {
        Self {
            code: 1001,
            reason: "heartbeat timeout",
            category: "heartbeat_timeout",
        }
    }

    pub(crate) const fn unsupported_data() -> Self {
        Self {
            code: 1003,
            reason: "unsupported data",
            category: "unsupported_data",
        }
    }

    pub(crate) const fn policy_violation() -> Self {
        Self {
            code: 1008,
            reason: "protocol violation",
            category: "protocol_violation",
        }
    }

    pub(crate) const fn session_revoked() -> Self {
        Self {
            code: 1008,
            reason: "session ended",
            category: "session_ended",
        }
    }

    pub(crate) const fn internal_error() -> Self {
        Self {
            code: 1011,
            reason: "internal error",
            category: "internal_error",
        }
    }

    pub(crate) const fn server_shutdown() -> Self {
        Self {
            code: 1012,
            reason: "server shutdown",
            category: "server_shutdown",
        }
    }

    pub(crate) const fn slow_consumer() -> Self {
        Self {
            code: 1013,
            reason: "resync required",
            category: "slow_consumer",
        }
    }
}

#[derive(Clone)]
pub(crate) struct RealtimeHub {
    inner: Arc<HubInner>,
}

impl fmt::Debug for RealtimeHub {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RealtimeHub")
            .field("settings", &self.inner.settings)
            .field(
                "registrations",
                &self.inner.registrations.load(Ordering::Relaxed),
            )
            .finish_non_exhaustive()
    }
}

struct HubInner {
    state: Mutex<HubState>,
    registrations: AtomicUsize,
    drained: Notify,
    settings: RealtimeSettings,
}

#[derive(Default)]
struct HubState {
    shutting_down: bool,
    next_connection_id: u64,
    connections: HashMap<ConnectionId, ConnectionEntry>,
    users: HashMap<UserId, HashSet<ConnectionId>>,
    subscriptions: HashMap<ConversationId, HashSet<ConnectionId>>,
    user_registrations: HashMap<UserId, usize>,
}

struct ConnectionEntry {
    user_id: UserId,
    session: SessionFingerprint,
    outbound: mpsc::Sender<Arc<str>>,
    close: watch::Sender<Option<CloseDirective>>,
    subscriptions: HashSet<ConversationId>,
}

type ConnectionChannels = (
    mpsc::Receiver<Arc<str>>,
    watch::Receiver<Option<CloseDirective>>,
);

pub(crate) struct ConnectionRegistration {
    hub: RealtimeHub,
    connection_id: ConnectionId,
    user_id: UserId,
    session: SessionFingerprint,
    expires_at: SystemTime,
    outbound: Option<mpsc::Receiver<Arc<str>>>,
    close: Option<watch::Receiver<Option<CloseDirective>>>,
}

impl fmt::Debug for ConnectionRegistration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConnectionRegistration")
            .field("connection_id", &self.connection_id)
            .field("user_id", &self.user_id)
            .field("session", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

impl ConnectionRegistration {
    pub(crate) const fn connection_id(&self) -> ConnectionId {
        self.connection_id
    }

    pub(crate) const fn user_id(&self) -> UserId {
        self.user_id
    }

    pub(crate) const fn session(&self) -> SessionFingerprint {
        self.session
    }

    pub(crate) const fn expires_at(&self) -> SystemTime {
        self.expires_at
    }

    pub(crate) fn take_channels(&mut self) -> Option<ConnectionChannels> {
        Some((self.outbound.take()?, self.close.take()?))
    }
}

impl Drop for ConnectionRegistration {
    fn drop(&mut self) {
        self.hub
            .release_registration(self.connection_id, self.user_id);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CapacityError {
    ShuttingDown,
    GlobalLimit,
    UserLimit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SubscribeResult {
    Added,
    AlreadySubscribed,
    LimitReached,
    ConnectionClosed,
}

impl RealtimeHub {
    pub(crate) fn new(settings: RealtimeSettings) -> Self {
        Self {
            inner: Arc::new(HubInner {
                state: Mutex::new(HubState {
                    next_connection_id: 1,
                    ..HubState::default()
                }),
                registrations: AtomicUsize::new(0),
                drained: Notify::new(),
                settings,
            }),
        }
    }

    pub(crate) fn settings(&self) -> RealtimeSettings {
        self.inner.settings
    }

    pub(crate) fn reserve(
        &self,
        user_id: UserId,
        session: SessionFingerprint,
        expires_at: SystemTime,
    ) -> Result<ConnectionRegistration, CapacityError> {
        let mut state = self.lock_state();
        if state.shutting_down {
            return Err(CapacityError::ShuttingDown);
        }
        if self.inner.registrations.load(Ordering::Acquire) >= self.inner.settings.max_connections {
            return Err(CapacityError::GlobalLimit);
        }
        if state.user_registrations.get(&user_id).copied().unwrap_or(0)
            >= self.inner.settings.max_connections_per_user
        {
            return Err(CapacityError::UserLimit);
        }

        let connection_id = next_connection_id(&mut state);
        let (outbound_tx, outbound_rx) = mpsc::channel(self.inner.settings.outbound_queue_capacity);
        let (close_tx, close_rx) = watch::channel(None);
        state.connections.insert(
            connection_id,
            ConnectionEntry {
                user_id,
                session,
                outbound: outbound_tx,
                close: close_tx,
                subscriptions: HashSet::new(),
            },
        );
        state
            .users
            .entry(user_id)
            .or_default()
            .insert(connection_id);
        *state.user_registrations.entry(user_id).or_default() += 1;
        self.inner.registrations.fetch_add(1, Ordering::AcqRel);
        drop(state);

        Ok(ConnectionRegistration {
            hub: self.clone(),
            connection_id,
            user_id,
            session,
            expires_at,
            outbound: Some(outbound_rx),
            close: Some(close_rx),
        })
    }

    pub(crate) fn subscribe(
        &self,
        connection_id: ConnectionId,
        conversation_id: ConversationId,
    ) -> SubscribeResult {
        let mut state = self.lock_state();
        let result = {
            let Some(connection) = state.connections.get_mut(&connection_id) else {
                return SubscribeResult::ConnectionClosed;
            };
            if connection.subscriptions.contains(&conversation_id) {
                SubscribeResult::AlreadySubscribed
            } else if connection.subscriptions.len()
                >= self.inner.settings.max_subscriptions_per_connection
            {
                SubscribeResult::LimitReached
            } else {
                connection.subscriptions.insert(conversation_id);
                SubscribeResult::Added
            }
        };
        if result == SubscribeResult::Added {
            state
                .subscriptions
                .entry(conversation_id)
                .or_default()
                .insert(connection_id);
        }
        result
    }

    pub(crate) fn unsubscribe(
        &self,
        connection_id: ConnectionId,
        conversation_id: ConversationId,
    ) -> bool {
        let mut state = self.lock_state();
        let removed = state
            .connections
            .get_mut(&connection_id)
            .is_some_and(|connection| connection.subscriptions.remove(&conversation_id));
        if removed {
            remove_from_index(&mut state.subscriptions, conversation_id, connection_id);
        }
        state.connections.contains_key(&connection_id)
    }

    pub(crate) fn publish_events(&self, events: &[ChatEvent]) {
        for event in events {
            match event {
                ChatEvent::ConversationCreated {
                    conversation,
                    creator_membership,
                } => {
                    let message = ServerMessage::conversation_created(conversation.id());
                    self.publish_to_user(creator_membership.user_id(), &message);
                }
                ChatEvent::MessagePosted { message } => {
                    let notification =
                        ServerMessage::message_posted(message.conversation_id(), message.id());
                    self.publish_to_conversation(message.conversation_id(), &notification);
                }
                ChatEvent::UserCreated { .. }
                | ChatEvent::MemberAdded { .. }
                | ChatEvent::MemberRemoved { .. } => {}
            }
        }
    }

    pub(crate) fn close_session(&self, session: SessionFingerprint, directive: CloseDirective) {
        let mut state = self.lock_state();
        let connection_ids = state
            .connections
            .iter()
            .filter_map(|(id, entry)| (entry.session == session).then_some(*id))
            .collect::<Vec<_>>();
        close_connections(&mut state, &connection_ids, directive);
    }

    pub(crate) fn shutdown(&self) {
        let mut state = self.lock_state();
        state.shutting_down = true;
        let connection_ids = state.connections.keys().copied().collect::<Vec<_>>();
        close_connections(
            &mut state,
            &connection_ids,
            CloseDirective::server_shutdown(),
        );
    }

    pub(crate) async fn wait_for_drain(&self) {
        loop {
            let notified = self.inner.drained.notified();
            if self.inner.registrations.load(Ordering::Acquire) == 0 {
                return;
            }
            notified.await;
        }
    }

    fn publish_to_user(&self, user_id: UserId, message: &ServerMessage) {
        let Ok(encoded) = encode_server_message(message) else {
            tracing::error!("realtime event serialization failed");
            return;
        };
        let mut state = self.lock_state();
        let connection_ids = state
            .users
            .get(&user_id)
            .map(|ids| ids.iter().copied().collect::<Vec<_>>())
            .unwrap_or_default();
        publish_locked(&mut state, &connection_ids, encoded);
    }

    fn publish_to_conversation(&self, conversation_id: ConversationId, message: &ServerMessage) {
        let Ok(encoded) = encode_server_message(message) else {
            tracing::error!("realtime event serialization failed");
            return;
        };
        let mut state = self.lock_state();
        let connection_ids = state
            .subscriptions
            .get(&conversation_id)
            .map(|ids| ids.iter().copied().collect::<Vec<_>>())
            .unwrap_or_default();
        publish_locked(&mut state, &connection_ids, encoded);
    }

    fn release_registration(&self, connection_id: ConnectionId, user_id: UserId) {
        let mut state = self.lock_state();
        remove_connection(&mut state, connection_id);
        if let Some(count) = state.user_registrations.get_mut(&user_id) {
            *count -= 1;
            if *count == 0 {
                state.user_registrations.remove(&user_id);
            }
        }
        drop(state);
        if self.inner.registrations.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.inner.drained.notify_waiters();
        }
    }

    fn lock_state(&self) -> MutexGuard<'_, HubState> {
        match self.inner.state.lock() {
            Ok(state) => state,
            Err(poisoned) => {
                tracing::error!("realtime hub lock was poisoned; recovering state");
                poisoned.into_inner()
            }
        }
    }
}

fn next_connection_id(state: &mut HubState) -> ConnectionId {
    loop {
        let id = ConnectionId(state.next_connection_id);
        state.next_connection_id = state.next_connection_id.wrapping_add(1).max(1);
        if !state.connections.contains_key(&id) {
            return id;
        }
    }
}

fn publish_locked(state: &mut HubState, ids: &[ConnectionId], message: Arc<str>) {
    let mut closed = Vec::new();
    let mut slow = Vec::new();
    for connection_id in ids {
        let Some(connection) = state.connections.get(connection_id) else {
            continue;
        };
        match connection.outbound.try_send(message.clone()) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Closed(_)) => closed.push(*connection_id),
            Err(mpsc::error::TrySendError::Full(_)) => slow.push(*connection_id),
        }
    }
    for connection_id in closed {
        remove_connection(state, connection_id);
    }
    for connection_id in slow {
        if let Some(connection) = state.connections.get(&connection_id) {
            let _ = connection
                .close
                .send_replace(Some(CloseDirective::slow_consumer()));
            tracing::warn!(%connection_id, "closing slow realtime consumer");
        }
        remove_connection(state, connection_id);
    }
}

fn close_connections(
    state: &mut HubState,
    connection_ids: &[ConnectionId],
    directive: CloseDirective,
) {
    for connection_id in connection_ids {
        if let Some(connection) = state.connections.get(connection_id) {
            let _ = connection.close.send_replace(Some(directive));
        }
        remove_connection(state, *connection_id);
    }
}

fn remove_connection(state: &mut HubState, connection_id: ConnectionId) {
    let Some(connection) = state.connections.remove(&connection_id) else {
        return;
    };
    remove_from_index(&mut state.users, connection.user_id, connection_id);
    for conversation_id in connection.subscriptions {
        remove_from_index(&mut state.subscriptions, conversation_id, connection_id);
    }
}

fn remove_from_index<K>(
    index: &mut HashMap<K, HashSet<ConnectionId>>,
    key: K,
    connection_id: ConnectionId,
) where
    K: Copy + Eq + std::hash::Hash,
{
    if let Some(ids) = index.get_mut(&key) {
        ids.remove(&connection_id);
        if ids.is_empty() {
            index.remove(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use chat::{Conversation, ConversationTitle, Membership, MembershipRole};

    use super::*;

    fn fingerprint(value: u8) -> SessionFingerprint {
        SessionFingerprint::new([value; 32])
    }

    fn reserve(hub: &RealtimeHub, user: i64) -> ConnectionRegistration {
        hub.reserve(
            UserId::new(user).unwrap(),
            fingerprint(user as u8),
            SystemTime::now() + Duration::from_secs(60),
        )
        .unwrap()
    }

    #[test]
    fn limits_and_subscription_indexes_are_bounded_and_idempotent() {
        let settings = RealtimeSettings {
            max_connections: 2,
            max_connections_per_user: 1,
            max_subscriptions_per_connection: 1,
            ..RealtimeSettings::default()
        };
        let hub = RealtimeHub::new(settings);
        let first = reserve(&hub, 1);
        assert_eq!(
            hub.reserve(
                UserId::new(1).unwrap(),
                fingerprint(2),
                SystemTime::now() + Duration::from_secs(60),
            )
            .unwrap_err(),
            CapacityError::UserLimit
        );
        let second = reserve(&hub, 2);
        assert_eq!(
            hub.reserve(
                UserId::new(3).unwrap(),
                fingerprint(3),
                SystemTime::now() + Duration::from_secs(60),
            )
            .unwrap_err(),
            CapacityError::GlobalLimit
        );

        let conversation = ConversationId::new(1).unwrap();
        assert_eq!(
            hub.subscribe(first.connection_id(), conversation),
            SubscribeResult::Added
        );
        assert_eq!(
            hub.subscribe(first.connection_id(), conversation),
            SubscribeResult::AlreadySubscribed
        );
        assert_eq!(
            hub.subscribe(first.connection_id(), ConversationId::new(2).unwrap()),
            SubscribeResult::LimitReached
        );
        drop(second);
        drop(first);
    }

    #[tokio::test]
    async fn publication_targets_subscribers_and_slow_consumers_are_closed() {
        let settings = RealtimeSettings {
            outbound_queue_capacity: 1,
            ..RealtimeSettings::default()
        };
        let hub = RealtimeHub::new(settings);
        let mut subscribed = reserve(&hub, 1);
        let mut other = reserve(&hub, 2);
        let (mut subscribed_rx, mut subscribed_close) = subscribed.take_channels().unwrap();
        let (mut other_rx, _other_close) = other.take_channels().unwrap();
        let conversation_id = ConversationId::new(1).unwrap();
        hub.subscribe(subscribed.connection_id(), conversation_id);

        let conversation = Conversation::new(
            conversation_id,
            ConversationTitle::try_from("General").unwrap(),
            SystemTime::now(),
        );
        let membership = Membership::new(
            conversation_id,
            UserId::new(1).unwrap(),
            MembershipRole::Owner,
            SystemTime::now(),
        );
        hub.publish_events(&[ChatEvent::ConversationCreated {
            conversation,
            creator_membership: membership,
        }]);
        assert!(
            subscribed_rx
                .recv()
                .await
                .unwrap()
                .contains("conversation_created")
        );
        assert!(other_rx.try_recv().is_err());

        let message = ServerMessage::subscribed(conversation_id);
        hub.publish_to_conversation(conversation_id, &message);
        hub.publish_to_conversation(conversation_id, &message);
        subscribed_close.changed().await.unwrap();
        assert_eq!(
            *subscribed_close.borrow(),
            Some(CloseDirective::slow_consumer())
        );
    }

    #[tokio::test]
    async fn shutdown_signals_connections_and_waits_for_registration_drop() {
        let hub = RealtimeHub::new(RealtimeSettings::default());
        let mut registration = reserve(&hub, 1);
        let (_, mut close) = registration.take_channels().unwrap();
        hub.shutdown();
        close.changed().await.unwrap();
        assert_eq!(*close.borrow(), Some(CloseDirective::server_shutdown()));
        assert_eq!(
            hub.reserve(
                UserId::new(2).unwrap(),
                fingerprint(2),
                SystemTime::now() + Duration::from_secs(60),
            )
            .unwrap_err(),
            CapacityError::ShuttingDown
        );
        drop(registration);
        hub.wait_for_drain().await;
    }

    #[tokio::test]
    async fn session_close_targets_only_the_matching_session() {
        let hub = RealtimeHub::new(RealtimeSettings::default());
        let user_id = UserId::new(1).unwrap();
        let expires_at = SystemTime::now() + Duration::from_secs(60);
        let mut first = hub
            .reserve(user_id, fingerprint(1), expires_at)
            .expect("first connection can be reserved");
        let mut second = hub
            .reserve(user_id, fingerprint(2), expires_at)
            .expect("second connection can be reserved");
        let (_, mut first_close) = first.take_channels().unwrap();
        let (_, second_close) = second.take_channels().unwrap();

        hub.close_session(fingerprint(1), CloseDirective::session_revoked());

        first_close.changed().await.unwrap();
        assert_eq!(
            *first_close.borrow(),
            Some(CloseDirective::session_revoked())
        );
        assert!(second_close.has_changed().is_ok_and(|changed| !changed));
    }

    #[test]
    fn close_reasons_fit_control_frame_payloads() {
        for directive in [
            CloseDirective::heartbeat_timeout(),
            CloseDirective::unsupported_data(),
            CloseDirective::policy_violation(),
            CloseDirective::session_revoked(),
            CloseDirective::internal_error(),
            CloseDirective::server_shutdown(),
            CloseDirective::slow_consumer(),
        ] {
            assert!(directive.reason.len() + 2 <= 125);
            assert!(directive.reason.is_ascii());
        }
    }
}
