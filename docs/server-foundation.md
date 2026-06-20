# Server Foundation Plan

Status: implemented and verified; formatting and manual SIGTERM checks remain
Date: 2026-06-21

## Scope

This milestone turns `chat-server` into a small, operational HTTP process. It
establishes configuration, diagnostics, startup, health handling, signal
handling, graceful shutdown, and deterministic SQLite cleanup.

It deliberately does not expose chat use cases. Authentication, application
protocol messages, WebSocket connections, and the web client remain later
milestones.

## Goals

- Start one Tokio runtime and one Axum HTTP server.
- Read a minimal, validated configuration from the process environment.
- Initialize structured diagnostics before opening runtime resources.
- Bind safely to loopback by default.
- Open and migrate SQLite before the server becomes ready.
- Expose separate liveness and readiness endpoints.
- Stop accepting new connections after SIGINT or SIGTERM.
- Wait for in-flight HTTP work, then explicitly close the SQLite pool.
- Keep `main.rs` as a small composition root.
- Make startup, routing, and shutdown behavior testable without process-global
  test mutation.

## Non-Goals

- authentication or authorization transport
- chat HTTP endpoints
- WebSocket upgrade or connection management
- event fan-out
- JSON request or response types
- serving or embedding the web client
- TLS termination
- CORS configuration
- compression
- request IDs or metrics
- a configuration file format or command-line parser
- a hard shutdown deadline for long-lived WebSocket connections

These exclusions are intentional. Adding them together would make lifecycle
failures difficult to isolate and would force protocol decisions before the
server process itself is established.

## Dependencies

Use the current stable releases researched for this milestone:

```toml
[workspace.dependencies]
axum = { version = "0.8.9", default-features = false, features = [
    "http1",
    "tokio",
    "tracing",
] }
tracing = { version = "0.1.44", default-features = false, features = ["std"] }
tracing-subscriber = { version = "0.3.23", features = ["env-filter"] }
tower-http = { version = "0.6.11", default-features = false, features = ["trace"] }
tokio = { version = "1.52.3", features = [
    "macros",
    "net",
    "rt-multi-thread",
    "signal",
    "sync",
    "time",
] }

[dev-dependencies]
tower = { version = "0.5.3", features = ["util"] }
```

Reasons:

- Axum uses Tokio and Tower directly, matches the future WebSocket server, and
  provides `serve(...).with_graceful_shutdown(...)` without another server
  wrapper.
- Only HTTP/1, Tokio integration, and Axum rejection tracing are enabled now.
  JSON and WebSocket features will be enabled when their protocols exist.
- `tracing` provides structured events suitable for asynchronous tasks.
- `tracing-subscriber` supplies human-readable output and strict `RUST_LOG`
  filtering.
- `tower-http` contributes only standardized HTTP request tracing.
- Tower's `util` feature is a test-only direct dependency for `oneshot` router
  tests.

Do not add `anyhow`, `thiserror`, `clap`, `config`, `dotenv`, `serde`, `reqwest`,
or `axum-server` in this milestone. The required behavior is small enough to
express with the standard library and typed local errors. Axum's own server is
sufficient because TLS and forced connection deadlines are not yet required.

## Configuration Contract

Use environment variables with explicit defaults:

| Variable | Default | Meaning |
| --- | --- | --- |
| `CHAT_LISTEN_ADDR` | `127.0.0.1:3000` | IP socket address for HTTP |
| `CHAT_DATABASE_PATH` | `chat.sqlite3` | Local SQLite file |
| `RUST_LOG` | `chat_server=info,tower_http=info` | tracing filter |

`CHAT_LISTEN_ADDR` is parsed as `std::net::SocketAddr`. Hostname resolution is
not part of the initial contract. Requiring an IP address makes startup
deterministic and avoids hidden DNS work.

The default is loopback, not `0.0.0.0`. Exposing an unauthenticated development
server to the network must be an explicit choice.

Read the database path with `std::env::var_os` and preserve it as a `PathBuf` so
non-Unicode paths remain usable. Reject an explicitly empty path. An unset value
uses the default.

Use a `Config` type with read-only accessors and a typed `ConfigError`. Parse all
configuration before opening resources.

### Configuration testing

The production constructor reads a snapshot of environment values and delegates
to a pure parser that accepts optional `OsString` values. Tests call the pure
parser directly.

Do not call `std::env::set_var` or `remove_var` in tests. They are unsafe in the
Rust 2024 edition and process-global mutation makes parallel tests unreliable.

## Diagnostics

The binary owns global subscriber initialization. Library modules emit tracing
events but must not install a subscriber.

Initialization behavior:

1. If `RUST_LOG` is unset, use `chat_server=info,tower_http=info`.
2. If it is set, parse it strictly with `EnvFilter`.
3. Invalid filter syntax is a startup error rather than being silently ignored.
4. Install the subscriber with `try_init`; do not panic if installation fails.
5. Use human-readable output. JSON logging is deferred until an operational
   consumer requires it.

Add `TraceLayer::new_for_http` to the router. Configure spans and completed
responses at useful levels without recording request or response headers. Header
logging risks exposing future cookies and credentials.

Emit structured application events for:

- configuration accepted
- listener bound
- SQLite opened and migrated
- server ready
- shutdown signal received
- HTTP serving stopped
- SQLite pool closed
- startup or shutdown failure

Use fields such as `listen_addr` and `database_path`, not values interpolated
into opaque strings.

## Startup Sequence

The startup order is fixed:

```text
initialize tracing
        |
parse and validate configuration
        |
open SQLite and run embedded migrations
        |
bind TcpListener
        |
construct application state and router
        |
log ready and begin serving
```

Opening and migrating SQLite before binding ensures the process does not accept
or queue TCP connections while its durable state is unavailable. If binding
then fails, explicitly close the already-open pool before returning the bind
error. This cleanup path is part of the lifecycle tests.

Startup errors retain their original sources in typed errors:

- `ConfigError` for invalid configuration
- a telemetry initialization error for invalid filters or subscriber setup
- `RunError` for SQLite open or migration failure
- `RunError` for listener bind failure, including the requested address
- `RunError` for an unexpected serving failure

The binary logs the complete error chain and exits with `ExitCode::FAILURE`.
Successful graceful shutdown returns `ExitCode::SUCCESS`.

## Application State and Router

Start with one private, cheaply cloned state value:

```text
AppState
`-- SqliteStore
```

Do not add `Chat<SqliteStore>` to state until a route invokes a core use case.
The health routes only require persistence readiness.

Routes:

| Method | Path | Success | Failure | Purpose |
| --- | --- | --- | --- | --- |
| `GET` | `/health/live` | `204` | none | process and router are running |
| `GET` | `/health/ready` | `204` | `503` | SQLite responds within one second |

Responses have no body. Unknown routes remain Axum's normal `404`, and unsupported
methods remain `405`. Do not introduce a custom error envelope before the
application protocol is designed.

### Liveness

Liveness does not access SQLite. A temporary database problem must not tell a
process supervisor to restart an otherwise functioning process repeatedly.

### Readiness

Readiness executes a minimal `SELECT 1` through the existing pool and wraps it in
a one-second Tokio timeout. It returns `503 Service Unavailable` when the pool is
closed, acquisition fails, the query fails, or the deadline expires.

Add a crate-private readiness method to `SqliteStore`; do not expose its pool or
SQLx errors through HTTP. The handler converts readiness to status only. Request
tracing records the failure status without producing one warning for every
probe.

## Shutdown Semantics

Listen for:

- portable Ctrl-C/SIGINT through `tokio::signal::ctrl_c`
- SIGTERM on Unix through `tokio::signal::unix`

Registering Tokio signal handling replaces the platform's default behavior for
those signals. Therefore the signal future must always lead to application
shutdown, including when signal registration or reception reports an error.

Pass the signal future to Axum's `with_graceful_shutdown`. When it resolves:

1. stop accepting new TCP connections
2. let Axum finish active HTTP requests
3. let the server future return
4. explicitly call `SqliteStore::close().await`
5. return from `run`

SQLx documents `Pool::close` as waking waiters, rejecting new acquisitions, and
waiting for checked-out connections to return before closing them. Calling it
after Axum drains requests gives handlers a consistent lifetime.

Axum graceful shutdown can wait indefinitely for a long-lived connection. This
is acceptable while only short health requests exist. Before WebSockets are
added, design connection broadcast shutdown and a bounded drain deadline.

## Module Layout

Keep all work in `chat-server`:

```text
crates/chat-server/src/
|-- app.rs          # AppState, router, health handlers
|-- config.rs       # Config and ConfigError
|-- lib.rs          # narrow public exports
|-- main.rs         # Tokio entry point and exit status
|-- server.rs       # bind, serve, signal, cleanup, RunError
|-- telemetry.rs    # binary-owned subscriber initialization
|-- sqlite.rs
`-- sqlite/
    |-- read.rs
    |-- row.rs
    `-- write.rs
```

`telemetry.rs` is declared by the binary rather than the library because global
subscriber installation is an executable concern. The library emits `tracing`
events and exposes only what `main.rs` needs: `Config`, `RunError`, and `run`.

Do not create another crate. There is still one executable, one runtime adapter,
and no second consumer that would justify another compilation boundary.

## Error Policy

- Startup failures are typed errors with `source()` chains.
- Handler-level health failure is represented only as HTTP `503`.
- Expected configuration errors do not panic.
- Signal registration failure initiates shutdown and is logged.
- Do not erase startup errors into strings before logging or returning them.
- Do not expose filesystem paths, SQL errors, or internal causes in HTTP bodies.
- Avoid `unwrap` and `expect` in production lifecycle code.

## Test Plan

### Unit tests

- default configuration
- each configuration override
- invalid and non-Unicode listen address
- empty database path
- tracing filter construction without installing a global subscriber
- startup error display and source preservation where useful

### Router tests

Use `tower::ServiceExt::oneshot`; do not start a real socket for handler tests.

- liveness returns `204`
- readiness returns `204` with an open migrated SQLite store
- readiness returns `503` after the pool is closed
- liveness remains `204` after the pool is closed
- unknown route returns `404`
- unsupported health method returns `405`

### Lifecycle tests

Inject a shutdown future into a private `run_until`/`serve_until` function so
tests do not send process signals.

- an immediately resolved shutdown exits successfully
- shutdown closes SQLite and the database can be reopened
- an occupied listen address returns the bind error
- bind failure explicitly closes SQLite and leaves a reopenable migrated file
- an invalid database location returns the SQLite startup error

Do not mutate process environment, install multiple global tracing subscribers,
or depend on test execution order.

### Manual verification

```sh
CHAT_DATABASE_PATH=/tmp/chat.sqlite3 \
RUST_LOG=chat_server=debug,tower_http=info \
cargo run --release -p chat-server

curl -i http://127.0.0.1:3000/health/live
curl -i http://127.0.0.1:3000/health/ready
```

Verify both return `204`, then test Ctrl-C. On Unix, also send SIGTERM and verify
the same orderly shutdown log sequence.

Run the repository checks:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo doc --workspace --no-deps
cargo build --workspace --release --locked
```

## Implementation Record

The milestone was implemented as the following reviewable slices.

### Slice 1: Dependencies and configuration

- add the exact dependency features above
- add `Config` and typed parsing errors
- document environment variables
- add pure configuration tests

Result: configuration has no I/O beyond reading a snapshot and its parsing
branches have focused unit tests.

### Slice 2: Diagnostics

- initialize `tracing-subscriber` in the binary
- use a strict `RUST_LOG` override and stable default
- add structured startup events
- add HTTP `TraceLayer` without header recording

Result: startup diagnostics are structured, filter errors fail clearly, and
tests construct filters without installing a global subscriber.

### Slice 3: Router and health

- add private application state
- add liveness and bounded readiness handlers
- add the SQLite readiness operation
- test the router through Tower `oneshot`

Result: route and status contracts are covered without a live TCP listener.

### Slice 4: Lifecycle and shutdown

- bind `TcpListener`
- serve the router with injected shutdown support
- handle SIGINT and Unix SIGTERM
- explicitly close SQLite after draining HTTP
- return typed errors and meaningful process exit status
- add lifecycle tests

Result: normal and failure startup paths are deterministic, shutdown is
testable without real signals, and runtime resources close in the documented
order.

### Slice 5: Verification and documentation

- run the full locked check suite and release build
- manually probe both health routes
- manually verify Ctrl-C and SIGTERM
- update README milestone status and operational examples

Result: documentation and operational examples are updated. Rust 1.96 check,
Clippy, workspace tests, release build, health probes, and graceful SIGINT
shutdown have been verified locally. Formatting and manual SIGTERM checks
remain.

## Acceptance Criteria

The milestone is complete when:

- `cargo run -p chat-server` starts with documented defaults
- invalid configuration, bind failure, and database failure exit nonzero
- migrations finish before readiness is exposed
- both health endpoint contracts are tested
- SIGINT and SIGTERM trigger graceful shutdown
- the SQLite pool is explicitly closed after HTTP draining
- startup and request lifecycle produce structured traces
- `main.rs` contains composition rather than application rules
- no authentication, chat endpoint, or WebSocket behavior has entered the scope
- all workspace checks and the release build pass with `--locked`

## Primary References

- [Axum 0.8.9 documentation](https://docs.rs/axum/0.8.9/axum/)
- [Axum `serve`](https://docs.rs/axum/0.8.9/axum/fn.serve.html)
- [Axum graceful shutdown](https://docs.rs/axum/0.8.9/axum/serve/struct.Serve.html#method.with_graceful_shutdown)
- [Tokio Ctrl-C handling](https://docs.rs/tokio/1.52.3/tokio/signal/fn.ctrl_c.html)
- [Tokio Unix signals](https://docs.rs/tokio/1.52.3/tokio/signal/unix/fn.signal.html)
- [Tracing 0.1.44](https://docs.rs/tracing/0.1.44/tracing/)
- [Tracing Subscriber 0.3.23](https://docs.rs/tracing-subscriber/0.3.23/tracing_subscriber/)
- [Tracing `EnvFilter`](https://docs.rs/tracing-subscriber/0.3.23/tracing_subscriber/filter/struct.EnvFilter.html)
- [Tower HTTP request tracing](https://docs.rs/tower-http/0.6.11/tower_http/trace/)
- [Tower `ServiceExt::oneshot`](https://docs.rs/tower/0.5.3/tower/trait.ServiceExt.html#method.oneshot)
- [SQLx pool shutdown](https://docs.rs/sqlx/0.9.0/sqlx/pool/struct.Pool.html#method.close)
- [Rust 2024 newly unsafe environment functions](https://doc.rust-lang.org/edition-guide/rust-2024/newly-unsafe-functions.html)
- [Rust `var_os`](https://doc.rust-lang.org/std/env/fn.var_os.html)
