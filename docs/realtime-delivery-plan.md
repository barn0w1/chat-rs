# Real-Time Delivery Foundation Plan

Status: planned; implementation has not started
Date: 2026-06-22
Baseline: `3d4374c`

## Decision Summary

Milestone 5A will add the first real-time delivery foundation to
`chat-server`:

- `GET /api/v1/ws` upgrades an authenticated, same-origin browser request;
- the client and server must negotiate the `chat.v1` WebSocket subprotocol;
- chat commands and durable queries remain on the existing HTTP API;
- client-to-server WebSocket messages are limited to conversation subscription
  control;
- server-to-client messages are small change notifications, not authoritative
  chat state;
- SQLite and the authenticated HTTP resources remain the durable source of
  truth;
- every connection has bounded subscriptions and a bounded outbound queue;
- a slow consumer is disconnected and must resynchronize instead of silently
  continuing after dropped notifications;
- server Ping/Pong heartbeats detect unresponsive connections; and
- shutdown actively closes WebSocket connections so Axum can finish graceful
  shutdown within a bounded interval.

This increment deliberately does not add a durable event log. A notification
can still be missed if the process stops after committing SQLite but before
publishing to the in-memory hub. The protocol therefore makes no lossless,
exactly-once, or global-ordering promise.

## Why This Is the Next Boundary

The current system already has committed mutation results and `ChatEvent`
values, authenticated actor-aware HTTP operations, keyset-paginated history,
server-side sessions, exact Origin validation, deterministic server lifecycle,
and production-like Google/Caddy/browser E2E evidence.

What is missing is a bounded runtime consumer for successful mutation events.
Adding an unbounded broadcast channel would hide the important decisions:

- which connected users may observe a conversation;
- what happens when a client cannot keep up;
- how a client closes the snapshot/subscription race;
- whether notifications can be replayed;
- how logout, expiry, and shutdown affect upgraded connections; and
- which data is safe to place in a non-durable notification.

Milestone 5A fixes those contracts before the browser client depends on them.

## Scope

Implement in this increment:

- Axum's `ws` feature and an authenticated `/api/v1/ws` route;
- mandatory `chat.v1` subprotocol negotiation;
- exact configured Origin validation before `101 Switching Protocols`;
- session-cookie authentication before upgrade;
- connection reservation and global/per-user limits;
- conversation subscribe and unsubscribe control messages;
- membership validation before accepting a subscription;
- in-memory connection and subscription indexes;
- bounded per-connection outbound queues;
- publication of existing conversation-created and message-posted events;
- server Ping/Pong heartbeat and zombie-connection timeout;
- session-expiry and logout-driven connection closure;
- slow-consumer and server-shutdown close behavior;
- safe structured connection lifecycle logging;
- unit, router, real-socket integration, and shutdown tests; and
- a manual smoke procedure for the existing Caddy deployment.

Do not implement in this increment:

- WebSocket chat commands or queries;
- message bodies or other durable state in notifications;
- a durable outbox, event table, global sequence, replay cursor, or resume
  token;
- delivery acknowledgements or exactly-once processing;
- HTTP mutation idempotency or automatic retry;
- membership invitation, user discovery, add-member, or remove-member HTTP
  workflows;
- cross-origin clients, CORS, a complete web UI, or embedded assets;
- editing, deletion, reactions, typing indicators, or presence;
- WebSocket compression, cluster-wide fan-out, or multiple server processes;
- general rate limiting, metrics export, or a plugin API.

The server remains one process using one SQLite database. A multi-process
design would require a different distribution mechanism and is not implied by
the in-memory hub.

## Delivery Guarantees

| Property | Milestone 5A contract |
| --- | --- |
| Durable state | SQLite and authenticated HTTP resources |
| Notification durability | None |
| Delivery while connected | Best effort while the connection remains healthy |
| Delivery after reconnect | No replay; fetch current state over HTTP |
| Duplicate notifications | Allowed |
| Notification ordering | Per-connection enqueue order; no global or commit-order guarantee |
| Slow consumer | Close and resynchronize; never silently drop while open |
| Authorization | Session at upgrade plus membership at subscription |
| Notification contents | Event kind and IDs only |

The HTTP mutation response remains authoritative for its caller. A WebSocket
notification only indicates that an authenticated HTTP read may return new
state.

## Endpoint and Opening Handshake

Use:

```text
GET /api/v1/ws
Sec-WebSocket-Protocol: chat.v1
Cookie: __Host-chat_session=...
Origin: https://chat.example.com
```

Loopback development retains the existing non-`__Host-` cookie name.

Before returning `101`, the handler:

1. parses the upgrade with Axum;
2. resolves exactly one valid session cookie;
3. requires exactly one `Origin` equal to `CHAT_PUBLIC_URL`'s origin;
4. requires the client to offer `chat.v1`;
5. reserves global and per-user capacity;
6. revalidates the session after reservation to close the logout/upgrade race;
7. configures strict frame, message, and write-buffer limits; and
8. installs a failed-upgrade callback that logs only safe context.

Authentication failure returns the existing `401` problem. Invalid or missing
Origin returns `403`. Missing or unsupported subprotocol returns `400`.
Exhausted capacity returns `503`. Failed handshakes remain
`Cache-Control: no-store`.

Use Axum's `any` routing rather than assuming only HTTP/1.1 `GET`. Axum
documents `GET` for HTTP/1.1 and `CONNECT` for later HTTP versions. The reverse
proxy may still translate a public connection to an HTTP/1.1 upstream upgrade.

Do not put session tokens, CSRF tokens, or bearer credentials in the URL or
subprotocol. The existing HttpOnly cookie is the continuity mechanism. Exact
Origin validation prevents a foreign page from using that ambient cookie.

## Wire Protocol

Application messages are UTF-8 JSON text. Binary messages are unsupported.
Every object has a required `type`, rejects unknown fields, and encodes IDs as
decimal strings like the HTTP API.

### Server ready

The first application message is:

```json
{"type":"ready"}
```

It means the connection is registered and accepts control messages. It does
not imply any conversation subscription.

### Subscribe

Client:

```json
{"type":"subscribe","conversation_id":"42"}
```

Server success:

```json
{"type":"subscribed","conversation_id":"42"}
```

The server parses a positive ID and calls the existing actor-aware
`Chat::get_conversation(user_id, conversation_id)`. Only a visible conversation
can be subscribed. Repeating a subscription is idempotent and returns the same
acknowledgement without using another slot.

Expected rejection is an in-band message:

```json
{
  "type":"subscription_rejected",
  "conversation_id":"42",
  "reason":"not_found"
}
```

Allowed reasons are `invalid_request`, `not_found`, `limit_reached`, and
`temporarily_unavailable`. Lack of membership and an absent conversation both
use `not_found`, preserving the HTTP non-disclosure rule. Invalid stored state
closes with `1011`; it is not a normal subscription result.

### Unsubscribe

Client:

```json
{"type":"unsubscribe","conversation_id":"42"}
```

Server:

```json
{"type":"unsubscribed","conversation_id":"42"}
```

Unsubscribing an inactive conversation is idempotent and performs no database
query.

### Conversation created

```json
{"type":"conversation_created","conversation_id":"42"}
```

This is sent directly to the creator's active connections so another tab can
refresh its conversation list. The initiating tab may see both the HTTP result
and notification and deduplicates by ID.

### Message posted

```json
{
  "type":"message_posted",
  "conversation_id":"42",
  "message_id":"99"
}
```

This is sent to every healthy connection subscribed to the conversation,
including the author's connections. It omits body and profile data. The
receiver obtains authoritative state from:

```text
GET /api/v1/conversations/42/messages/99
```

That HTTP read repeats current membership authorization. A stale subscription
cannot be used to read message content after access changes.

### Protocol violations

Malformed JSON, unknown `type` or fields, binary data, and out-of-bound values
close the connection. Log only a safe error category, never the peer payload.
Ping, Pong, and Close remain WebSocket control frames rather than JSON.

## Snapshot and Reconnect Algorithm

For each conversation the browser client must:

1. connect with `new WebSocket(url, "chat.v1")`;
2. wait for `ready`;
3. send `subscribe`;
4. wait for `subscribed`;
5. fetch the newest message page over HTTP; and
6. merge snapshot results and later notifications by message ID.

This closes the subscription/snapshot race:

- a message committed before acknowledgement appears in the snapshot;
- a message published after acknowledgement is queued; and
- a message seen through both paths is harmless because IDs are stable.

After disconnect, repeat the sequence. Never assume the final observed
notification was the final commit. Reconnect backoff belongs to the web-client
milestone, but server tests must prove reconnect and resubscribe recovery.

There remains a crash window between commit and publication. The initial web
client should refresh on reconnect and page focus. If durable continuous replay
becomes required, add an SQLite outbox rather than changing the documented
guarantee without storage.

## Runtime Design

### RealtimeHub

Add one cloneable `RealtimeHub` to `AppState`. Its state owns:

```text
connections: ConnectionId -> ConnectionEntry
users: UserId -> set of ConnectionId
subscriptions: ConversationId -> set of ConnectionId
```

Each entry contains user ID, non-secret session fingerprint, bounded outbound
sender, close-signal sender, and subscribed conversation IDs.

Use a short synchronous critical section. Never await, serialize JSON, query
SQLite, or write a socket while holding the hub lock. The hub exposes narrow
operations to reserve, attach, unregister, subscribe, unsubscribe, publish to a
user/conversation, close a session, begin shutdown, and inspect test counts.
A registration guard unregisters on normal return and task drop; cleanup is
idempotent.

### One owner task per socket

One task owns each Axum `WebSocket`. Do not initially split it into detached
reader and writer tasks. One `tokio::select!` loop coordinates:

- `WebSocket::recv()`;
- the bounded outbound receiver;
- close signal;
- heartbeat interval and pending-Pong deadline; and
- absolute session-expiry deadline.

This keeps closing and registration cleanup in one place. Axum has direct
async `recv` and `send`, so production does not need `futures-util`.

### Publication boundary

Publish only after a successful core use case has returned and its SQLite
transaction has committed. Publication never changes a committed HTTP success
into an error: doing that would invite an unsafe retry of a non-idempotent POST.

The hub uses `try_send` for every connection. An empty audience is normal, a
closed queue removes stale registration, and a full queue closes the slow
consumer.

Consume existing `ChatEvent` values at this composition boundary:

- `ConversationCreated` publishes to the creator;
- `MessagePosted` publishes to conversation subscribers; and
- `UserCreated`, `MemberAdded`, and `MemberRemoved` have no reachable HTTP
  mutation and gain no speculative transport behavior.

Keep the match exhaustive so a new core event forces an explicit decision.
Membership-event semantics belong to the separate membership workflow.

## Resource Bounds and Backpressure

Use fixed initial values in one injectable `RealtimeSettings`:

| Resource | Initial limit |
| --- | ---: |
| Live connections | 1024 per process |
| Live connections per user | 8 |
| Subscriptions per connection | 128 |
| Outbound application messages per connection | 64 |
| Reassembled inbound message | 4096 bytes |
| Inbound frame | 4096 bytes |
| WebSocket read buffer | 16 KiB |
| WebSocket write buffer target | 16 KiB |
| WebSocket maximum write buffer | 64 KiB |
| Close-handshake grace | 2 seconds |
| Whole-server drain | 10 seconds |

These are not compatibility promises. Test them before exposing selected values
as operator configuration.

Do not use an unbounded channel. Tokio documents that an unbounded receiver can
fall behind until process memory is exhausted. When a bounded queue is full:

1. remove the connection from every index;
2. signal `1013` (`Try Again Later`);
3. discard queued application notifications; and
4. require reconnect and HTTP resynchronization.

Do not drop one notification and leave the socket apparently healthy. With no
durable sequence, the client could not detect the gap.

## Heartbeat

Use server Ping control frames:

- first Ping after 30 seconds;
- one outstanding Ping at a time;
- an 8-byte monotonically changing payload;
- accept only a Pong with the identical outstanding payload;
- 90-second timeout from Ping transmission; and
- `MissedTickBehavior::Delay`, avoiding catch-up Ping bursts.

If no matching Pong arrives, close with `1001` and fixed reason
`heartbeat timeout`. Text traffic does not substitute for the matching Pong.
RFC 6455 defines Ping as keepalive/responsiveness detection and requires a
response Pong to repeat its data.

Axum/tungstenite automatically responds to client Ping. The loop still consumes
control messages. Browsers handle Pong below the JavaScript API. Explicit
heartbeat jitter can follow measurement if coordinated reconnects cause load.

## Session Lifecycle

Extend the internal authenticated session with a non-secret session fingerprint
derived from the stored token hash and its absolute expiry. The raw bearer
token remains confined to cookie parsing and never enters the hub, protocol,
logs, or `Debug` output.

Reserve by session fingerprint, then revalidate before accepting the upgrade:

- deletion before reservation is caught by revalidation; and
- deletion after reservation is observed through the hub session index.

Successful logout closes every connection for that exact session with `1008`.
Replacing a prior session during login does the same after the database change.
Other sessions for the same user remain active.

Each socket also has a local expiry deadline and closes with `1008` when
reached. This avoids periodic SQLite polling per connection. Direct external
deletion of session rows is not an administrative revocation API; a future
command must notify the hub through the same server boundary.

## Closing and Shutdown

Use registered close codes and fixed short ASCII reasons:

| Code | Use |
| ---: | --- |
| `1000` | normal completion |
| `1001` | heartbeat timeout or ordinary endpoint departure |
| `1003` | binary application message |
| `1008` | invalid protocol, revoked session, or expired session |
| `1009` | oversized frame/message |
| `1011` | internal invariant or serialization failure |
| `1012` | server shutdown/restart |
| `1013` | capacity pressure or slow consumer |

On SIGINT or SIGTERM:

1. mark the hub shutting down so no reservation succeeds;
2. signal Axum graceful shutdown;
3. signal every socket to send `1012`;
4. wait up to 2 seconds for peer close and then drop;
5. wait at most 10 seconds for HTTP/WebSocket drain; and
6. close SQLite only after serving stops.

The current foundation notes Axum can otherwise wait forever for an upgraded
connection. Refactor `run_until` so tests inject the shutdown future and short
realtime drain settings. A drain timeout is an error, not an infinite wait.

## Security and Privacy Rules

- Require exact Origin before upgrade, as recommended by RFC 6455.
- Use the existing session cookie; do not invent a WebSocket bearer token.
- Do not require the CSRF token: browser WebSocket constructors cannot set that
  custom header, and exact Origin protects the handshake.
- Never accept credentials in query strings or subprotocol values.
- Require `chat.v1`; never silently speak an unspecified protocol.
- Keep client masking enforcement enabled.
- Bound frames and reassembled messages against memory exhaustion.
- Strictly deserialize text and reject binary application data.
- Never log queries, headers, cookies, control payloads, JSON frames, message
  bodies, or peer-supplied close reasons.
- Log generated connection ID, local user ID, duration, subscription count,
  server-selected close category, and safe error class only.
- Send identifiers rather than message contents; HTTP repeats authorization.
- Continue deriving expected Origin from `CHAT_PUBLIC_URL`, not forwarding
  headers.

## Dependency Decision

Production changes:

- enable Axum 0.8.9's existing `ws` feature;
- use the already-enabled Tokio `sync` and `time` features; and
- add no new production crate.

Axum provides upgrade extraction, protocol negotiation, frame/message limits,
and direct `WebSocket::recv`/`send`. Tokio's bounded `mpsc` provides each
connection queue; Tokio timers provide heartbeat, expiry, and drain deadlines.

Test-only changes:

- `tokio-tungstenite = "0.29.0"` for real client handshakes and frames; and
- `futures-util = "0.3.32"` for the test client's `Stream`/`Sink` helpers.

Axum 0.8.9 itself uses `tokio-tungstenite` 0.29 in its tests. Keeping it
dev-only avoids adding a second production WebSocket stack. Resolve and commit
the resulting lockfile during implementation.

Do not add `tokio-util` only for cancellation. The explicit hub close signal
and existing Tokio synchronization primitives cover this lifecycle.

## Expected Module Changes

```text
Cargo.toml
crates/chat-server/Cargo.toml
crates/chat-server/src/
|-- app.rs                         # RealtimeHub in AppState
|-- auth.rs                        # session fingerprint and expiry
|-- auth/store.rs                  # resolve/revalidate fingerprint
|-- http.rs                        # merge realtime route
|-- http/authentication.rs         # authenticated same-origin WS context
|-- http/conversation.rs           # publish committed core events
|-- http/realtime.rs               # handshake and HTTP rejections
|-- realtime.rs                    # narrow internal module surface
|-- realtime/connection.rs         # socket owner and heartbeat
|-- realtime/hub.rs                # bounded registrations/subscriptions
|-- realtime/protocol.rs           # strict JSON control/event types
|-- realtime/tests.rs              # hub and real-socket tests
`-- server.rs                      # coordinated bounded shutdown
README.md
docs/authentication-protocol.md
docs/realtime-delivery-plan.md
Cargo.lock
```

No `chat` crate change and no migration are expected. If implementation finds
that either is required, stop and revise the plan rather than hiding it inside
the transport adapter.

## Verification Plan

### Protocol unit tests

- IDs use decimal strings and reject zero, negative, overflow, and non-decimal
  input;
- unknown `type`, unknown fields, binary input, and oversized messages fail;
- every server message serializes to the documented shape;
- subscribe and unsubscribe are idempotent; and
- fixed close reasons fit WebSocket control-frame limits.

### Hub unit tests

- global and per-user limits are atomic;
- dropping a registration removes user and conversation indexes;
- subscription limits are enforced;
- publication reaches only the intended user or conversation subscribers;
- one publication does not duplicate delivery to one connection;
- a full queue removes and closes only the slow connection;
- healthy peers continue after another peer is removed;
- revocation selects only that session's connections; and
- shutdown rejects reservations and signals all live connections.

### Router and authentication tests

- valid session, exact Origin, and `chat.v1` produce `101`;
- missing/invalid session produces `401`;
- missing, duplicate, malformed, or wrong Origin produces `403`;
- absent or unsupported subprotocol produces `400`;
- capacity exhaustion produces `503` before upgrade;
- handshake failures are non-cacheable;
- tracing records `/api/v1/ws` without headers or query values; and
- failed-upgrade logging contains no secret material.

### Real-socket integration tests

Run Axum on `127.0.0.1:0` and connect with `tokio-tungstenite`. Every network
read has an explicit timeout.

- receive `ready`, subscribe, and receive `subscribed`;
- reject a conversation invisible to the user;
- connect two authorized members, subscribe both, post through the HTTP
  handler, and observe the same `message_posted` ID on both sockets;
- confirm an unsubscribed connection receives no event;
- confirm duplicate subscription produces one event;
- fetch the notified message through HTTP and obtain the committed body;
- reconnect, resubscribe, and recover the newest page through HTTP;
- answer Ping correctly and keep the connection alive;
- withhold Pong and observe heartbeat close;
- send malformed text and binary data and observe documented close codes;
- log out and observe only that session's sockets close; and
- expire a session with injected settings and observe closure.

### Shutdown test

Extend the injected-shutdown server test:

1. start a real listener;
2. open an authenticated WebSocket;
3. trigger the shutdown future;
4. observe `1012` or connection completion within the deadline;
5. await `run_until`; and
6. reopen SQLite successfully.

Avoid sleeps as synchronization. Use ready messages, channels, injectable
durations, and `tokio::time::timeout` for deterministic failures.

### Mechanical gate

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --release --locked
```

### Manual Caddy/browser smoke test

After the mechanical gate:

1. log in through the existing OIDC flow;
2. connect from the same-origin page using `chat.v1`;
3. verify upgrade through Caddy;
4. subscribe two browser profiles to a seeded conversation;
5. post by HTTP in one and observe notification plus authorized fetch in the
   other;
6. restart the server and verify reconnect plus HTTP rebuild;
7. confirm application/Caddy policy does not record cookies or frame payloads;
8. confirm SIGTERM completes and SQLite reopens.

Serve the smoke helper from the configured origin. A developer-console script
on a browser-internal page can produce `Origin: null` and is not a valid
same-origin test.

## Implementation Slices

### Slice 1: protocol and bounded hub

- enable features and dev dependencies;
- add strict protocol DTOs;
- implement reservations, indexes, subscriptions, and queue policy;
- add pure unit tests.

### Slice 2: authenticated upgrade

- add route and subprotocol requirement;
- add session fingerprint/expiry data;
- validate session and Origin before upgrade;
- add capacity and handshake tests.

### Slice 3: connection lifecycle

- implement the single-owner socket loop;
- add Ping/Pong, session expiry, close handling, and safe logs;
- add real-socket protocol tests.

### Slice 4: committed event publication

- translate the two reachable core events;
- publish after successful HTTP mutations;
- prove two-client fan-out and HTTP reconciliation.

### Slice 5: logout and shutdown integration

- close sockets for a revoked session;
- coordinate realtime shutdown with Axum;
- add bounded drain and database-reopen tests;
- update current documentation and run the full gate.

Each slice must compile and pass focused tests before proceeding. Never leave
an unbounded or unauthenticated temporary WebSocket route between slices.

## Completion Criteria

Milestone 5A is complete only when:

- no unauthenticated or wrong-origin request can upgrade;
- `chat.v1` is mandatory and echoed;
- all application input and in-memory queues have explicit bounds;
- subscription authorization uses the actor-aware core query;
- committed message events reach healthy subscribers;
- slow consumers disconnect rather than silently desynchronize;
- reconnect plus HTTP snapshot recovers current messages;
- logout and expiry terminate affected sockets;
- heartbeat closes an unresponsive peer;
- SIGINT/SIGTERM drain WebSockets within the bound;
- no frame body, cookie, query credential, or secret is logged;
- no application rule enters transport handlers;
- mechanical and real-socket tests pass; and
- the reverse-proxy smoke result is recorded.

## Deferred Decisions

### Durable event replay

If hints are insufficient, add an outbox written in the same SQLite transaction
as each mutation. Specify retention, visibility, monotonic cursors,
acknowledgement, compaction, and transaction boundaries. An in-memory sequence
cannot close the crash window.

### Membership workflow

Adding/removing members needs a separate product and privacy design. Realtime
membership events, immediate subscription revocation, and invite UX belong to
that workflow rather than this transport increment.

### Mutation idempotency

Automatic POST retry requires a persisted operation key and response replay in
the mutation transaction. WebSocket notification does not solve it.

### Presence and typing

These are ephemeral state with different timeout, fan-out, and privacy rules.
Do not represent them as durable `ChatEvent` values in 5A.

### Operational tuning

Expose limits as environment configuration only after tests and deployment
measurements identify operator needs. Metrics and general rate limiting remain
operational-hardening work.

## Researched References

- [RFC 6455: The WebSocket Protocol](https://www.rfc-editor.org/rfc/rfc6455)
  - handshake, Origin, subprotocol, control frames, closing, limits, and
    cookie-based authentication
- [IANA WebSocket Protocol Registries](https://www.iana.org/assignments/websocket/websocket.xhtml)
  - registered close-code meanings
- [WHATWG WebSockets Standard](https://websockets.spec.whatwg.org/)
  - browser constructor, subprotocol API, Fetch credentials, and browser
    Ping/Pong behavior
- [Axum 0.8.9 `WebSocketUpgrade`](https://docs.rs/axum/0.8.9/axum/extract/ws/struct.WebSocketUpgrade.html)
  - upgrade, subprotocol selection, limits, and failed-upgrade callback
- [Axum 0.8.9 `WebSocket`](https://docs.rs/axum/0.8.9/axum/extract/ws/struct.WebSocket.html)
  - direct asynchronous send/receive
- [Axum 0.8.9 `Message`](https://docs.rs/axum/0.8.9/axum/extract/ws/enum.Message.html)
  - data/control messages and graceful close behavior
- [Tokio 1.52.3 bounded `mpsc`](https://docs.rs/tokio/1.52.3/tokio/sync/mpsc/fn.channel.html)
  - finite capacity, ordering, and backpressure
- [Tokio 1.52.3 `MissedTickBehavior`](https://docs.rs/tokio/1.52.3/tokio/time/enum.MissedTickBehavior.html)
  - avoiding catch-up heartbeat bursts
- [tokio-tungstenite 0.29](https://docs.rs/tokio-tungstenite/0.29.0/tokio_tungstenite/)
  - real asynchronous client used only in integration tests
