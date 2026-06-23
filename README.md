# chat-rs

Simple, reliable, self-hosted chat in Rust.

The intended deployment is a single server binary with a browser client. The
project is in early development and its public interfaces are not yet stable.

## Goals

- Keep deployment and operation simple.
- Serve a browser-first client from one server binary.
- Use WebSocket connections for real-time delivery.
- Keep application behavior independent of HTTP, WebSocket, authentication,
  and concrete database libraries.
- Make durability and reconnect behavior explicit rather than relying on an
  in-memory event stream as a source of truth.

## Workspace

- `crates/chat`: application types, use cases, events, and required storage
  capabilities
- `crates/chat-server`: executable server and runtime integrations
- `web`: browser client

Dependencies point inward: `chat-server` depends on `chat`; `chat` does not
depend on server or infrastructure crates. This is a practical module boundary,
not an attempt to implement a particular named architecture.

## Current Status

The first implementation of the `chat` crate is complete and is now being
stabilized. It currently provides:

- positive, strongly typed integer IDs for users, conversations, and messages
- users, conversations, memberships, roles, and messages
- validated display names, conversation titles, and message bodies
- mutation use cases for creating users and conversations, adding and removing
  members, and posting messages
- keyset-paginated query use cases for reading conversations, members, and
  messages
- application events returned by successful mutations
- small storage capability traits grouped into read and write stores
- use-case and storage-contract integration tests

Mutations return changed state and application events. Queries return read
models and do not emit events. Transport encoding, event delivery, authorization
credentials, transactions, and persistence implementations remain outside the
core crate.

Integer IDs are the canonical representation inside Rust and the database.
When they cross a JSON boundary, they will be encoded as strings so browser
clients do not lose precision.

## Roadmap

The milestones describe direction rather than a fixed release schedule.

### 1. Core application

Implement and stabilize the transport-independent `chat` crate. Keep the API
small, verify its contracts through tests, and revise abstractions only when an
actual adapter exposes a problem.

Status: implemented and verified with Rust 1.96 formatting, Clippy, tests, and
documentation builds; API stabilization remains.

### 2. SQLite persistence

Add schema migrations and a SQLite-backed implementation of the storage
capabilities in `chat-server`. Mutating operations that span multiple records
must be transactional. Integration tests will run the same behavioral contracts
against a real temporary database.

Status: implemented and verified with Rust 1.96 using locked Clippy, workspace
tests, and a release build. The implementation uses SQLx, bundled SQLite,
embedded migrations, atomic write transactions, and real-file integration tests.

### 3. Server foundation

Establish the production async runtime and server lifecycle, including
configuration, structured logging, health endpoints, graceful shutdown, and a
thin composition root. The executable should assemble the core use cases and
infrastructure without moving application rules into HTTP handlers.

Status: implemented and locally verified with Rust 1.96. The server reads
validated environment configuration, initializes structured tracing, opens and
migrates SQLite before binding, exposes liveness and readiness probes, and
closes the database pool after Axum graceful shutdown. Check, Clippy, all
workspace tests, release build, formatting, health probes, and graceful SIGINT
and SIGTERM shutdown pass.

### 4. Authentication and protocol

Define the same-origin browser authentication flow and a versioned wire
protocol. HTTP requests will be translated into core use-case commands and
queries. A later WebSocket channel will deliver live results and events.

Plan: implement this milestone in two increments. First establish a
method-independent verified-identity boundary, an OIDC adapter, server-side
sessions, CSRF protection, and `/api/v1` JSON conventions. Then expose the chat
use cases through authenticated HTTP routes. WebSocket remains a later live
event channel rather than a second command protocol.

Status: increments 4A, 4B.1, 4B.2, and server admission are implemented. The
server has a method-independent verified identity boundary, standards-based OIDC
authorization-code login with PKCE, SQLite-backed opaque sessions, secure
cookie policy, Origin and CSRF checks, and authenticated read resources for
conversations, members, and messages. Authenticated clients can create
conversations, post messages, and retrieve individual messages through strict,
bounded JSON routes. Production-like Google/Caddy/browser verification passed
all planned cases on 2026-06-22. New verified identities are admitted according
to `open` or `invite_only`; the latter accepts reusable, expiring codes created
by the server operator.

### 5. Real-time delivery

Add WebSocket connection management, bounded outbound queues, backpressure,
heartbeat timeouts, and event fan-out. SQLite remains the durable source of
truth so clients can recover missed state after reconnecting.

Status: the first implementation increment is implemented, verified by the
workspace CI gate, and production-like E2E verified through Caddy. It adds an
authenticated, same-origin `/api/v1/ws` endpoint, mandatory `chat.v1`
subprotocol negotiation, bounded subscriptions and outbound queues, small
change notifications, heartbeat and session lifecycle handling, and bounded
shutdown. HTTP remains the command and durable-query protocol; WebSocket does
not promise durable replay.

### 6. Web client and single-binary packaging

Implement the browser client, reconnect and history synchronization, and embed
the production web assets in `chat-server` so one binary is sufficient to run
the application.

### 7. Operational hardening

Add resource limits, observability, migration and backup procedures, security
review, compatibility tests, and reproducible release builds.

## Current Focus

The core application, SQLite persistence, and server foundation are implemented
and verified. The implementation decisions are recorded in
[`docs/sqlite-persistence.md`](docs/sqlite-persistence.md) and
[`docs/server-foundation.md`](docs/server-foundation.md).

The authentication and versioned HTTP protocol foundation is implemented and
mechanically verified. Its design and implemented contract are recorded in
[`docs/authentication-protocol.md`](docs/authentication-protocol.md).

The authenticated HTTP API plan is recorded in
[`docs/http-chat-api-plan.md`](docs/http-chat-api-plan.md). Increment 4B.1 adds
the shared HTTP boundary and authenticated, paginated read routes. Increment
4B.2 adds conversation creation, message posting, individual message retrieval,
strict request DTOs, body limits, Origin and CSRF enforcement, and finite
domain-error mapping. Its reviewed implementation plan is recorded in
[`docs/http-chat-mutations-plan.md`](docs/http-chat-mutations-plan.md). Retry
deduplication and the membership workflow remain separately planned work.

The server-admission increment separates verified identity from permission to
use a particular self-hosted server, adds explicit
`open` and `invite_only` policies, and keeps provider-specific authentication
outside the policy boundary. `invite_only` uses reusable, expiring admission
codes so an operator can share one code with a group; with no valid code it is
effectively closed. Its implemented scope, security contracts, persistence
design, and verification plan are recorded in
[`docs/server-admission-plan.md`](docs/server-admission-plan.md).

Production-like verification followed a focused source audit and hardening
increment. The reviewed findings, the path-only logging requirement, the
decision to retain server-side OpenID Connect Authorization Code Flow, and the
implementation gate are recorded in
[`docs/pre-e2e-audit-plan.md`](docs/pre-e2e-audit-plan.md).
The application now logs method and matched path without queries, makes
authentication redirects non-cacheable, rejects ambiguous security cookies,
bounds callback values, and limits pending login transactions. The
Google/Caddy/browser procedure in
[`docs/e2e-verification.md`](docs/e2e-verification.md) and its
[`redacted result`](docs/e2e-verification-report-2026-06-22.md) document the
completed E2E gate.

The real-time delivery foundation is implemented. Its reviewed scope, wire
contract, recovery model, resource bounds, lifecycle design, and verification
plan are recorded in
[`docs/realtime-delivery-plan.md`](docs/realtime-delivery-plan.md). WebSocket
notifications are recoverable hints over SQLite-backed HTTP state, not a second
command protocol or durable event log. The production-like realtime E2E plan
and temporary browser harness are recorded in
[`docs/realtime-e2e-verification.md`](docs/realtime-e2e-verification.md), with
the redacted result in
[`docs/realtime-e2e-verification-report-2026-06-23.md`](docs/realtime-e2e-verification-report-2026-06-23.md).

## Development

The repository toolchain is selected by `rust-toolchain.toml`.

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo doc -p chat --no-deps
```

Run the server with loopback and file-database defaults:

```sh
cargo run -p chat-server
```

Configuration is provided through `CHAT_LISTEN_ADDR`, `CHAT_DATABASE_PATH`,
`CHAT_PUBLIC_URL`, `CHAT_ADMISSION_MODE`, and `RUST_LOG`.
`CHAT_ADMISSION_MODE` is `invite_only` by default and also accepts `open`.
OIDC login is enabled by setting
`CHAT_OIDC_ISSUER` and `CHAT_OIDC_CLIENT_ID` together; a provider that requires
confidential-client authentication also uses `CHAT_OIDC_CLIENT_SECRET`.
The issuer is an exact OIDC identifier and is preserved as configured; do not
add or remove a trailing slash from the value published by the provider.
For example:

```sh
CHAT_LISTEN_ADDR=127.0.0.1:4000 \
CHAT_DATABASE_PATH=var/chat.sqlite3 \
CHAT_PUBLIC_URL=https://chat.example.com \
CHAT_OIDC_ISSUER=https://accounts.example.com \
CHAT_OIDC_CLIENT_ID=chat \
RUST_LOG=chat_server=debug,tower_http=info \
cargo run -p chat-server
```

Create a shared admission code with a bounded lifetime. The command prints the
raw code once and stores only its hash. It may run in a separate process while
the server is running; both processes must use the same `CHAT_DATABASE_PATH`.
The running server reads admission codes from SQLite and does not require a
restart.

```sh
CHAT_DATABASE_PATH=var/chat.sqlite3 \
cargo run -p chat-server -- admission-code create --valid-for-hours 168
```

The operational probes are `GET /health/live` and `GET /health/ready`; both
return `204 No Content` when the process is ready.

## License

No license is granted for this repository.
