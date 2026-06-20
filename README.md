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
- query use cases for reading conversations, members, and paginated messages
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

### 4. Authentication and protocol

Define the same-origin browser authentication flow and a versioned wire
protocol. HTTP and WebSocket inputs will be translated into core use-case
requests; results and events will be translated into client messages.

### 5. Real-time delivery

Add WebSocket connection management, bounded outbound queues, backpressure,
heartbeat timeouts, and event fan-out. SQLite remains the durable source of
truth so clients can recover missed state after reconnecting.

### 6. Web client and single-binary packaging

Implement the browser client, reconnect and history synchronization, and embed
the production web assets in `chat-server` so one binary is sufficient to run
the application.

### 7. Operational hardening

Add resource limits, observability, migration and backup procedures, security
review, compatibility tests, and reproducible release builds.

## Current Focus

The SQLite persistence implementation is complete and verified. Its decisions
are recorded in
[`docs/sqlite-persistence.md`](docs/sqlite-persistence.md).

The next milestone is the server foundation. The proposed implementation plan
is recorded in
[`docs/server-foundation.md`](docs/server-foundation.md).

Before adding application endpoints or WebSocket protocol messages:

1. Define configuration for the listen address and database path.
2. Establish the Tokio process lifecycle and a thin composition root.
3. Initialize structured tracing before opening runtime resources.
4. Open and migrate SQLite during startup, failing startup on errors.
5. Add health handling and graceful shutdown with deterministic resource cleanup.
6. Verify startup, shutdown, and configuration behavior without introducing the
   authentication or WebSocket protocol yet.

This milestone is complete when the server starts with documented defaults,
reports liveness and readiness, shuts down cleanly on supported signals, closes
SQLite deterministically, and passes the full workspace checks. It deliberately
excludes authentication, chat endpoints, and WebSocket behavior.

## Development

The repository toolchain is selected by `rust-toolchain.toml`.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo doc --workspace --no-deps
```