# SQLite Persistence Plan

Status: accepted for the first `chat-server` persistence implementation
Date: 2026-06-20

## Scope

This document fixes the main implementation decisions for the next milestone.
The milestone adds a durable SQLite implementation of every storage capability
required by `chat`. It does not add HTTP, WebSocket, authentication, or event
fan-out.

## Decisions

### Runtime and database library

Use Tokio and SQLx 0.9 with the SQLite driver.

- SQLx exposes an async API and moves SQLite's blocking C API work to a worker
  thread for each connection.
- SQLx implements the pool, transactions, typed row decoding, and migrations
  needed by this project without introducing an ORM.
- SQLx 0.9 declares Rust 1.94 as its minimum supported Rust version, which is
  compatible with this workspace's Rust 1.96 toolchain.
- Tokio is also the intended runtime for the later Axum server, so a separate
  persistence executor is unnecessary.
- Do not combine SQLx with `rusqlite` or an async `rusqlite` wrapper. One SQLite
  access path is simpler and avoids native-link dependency conflicts.

Use narrow dependency features. The intended starting point is equivalent to:

```toml
[workspace.dependencies]
sqlx = { version = "0.9.0", default-features = false, features = [
    "macros",
    "migrate",
    "runtime-tokio",
    "sqlite-bundled",
] }
tokio = { version = "1.52.3", features = ["macros", "rt-multi-thread"] }
```

`sqlite-bundled` makes the server binary independent of a system SQLite
installation. More Tokio features will be added only when the server needs
them.

### SQL API

Use parameterized SQL with `query`, `query_as`, and private row structs. Do not
use an ORM.

For the first adapter, do not use SQLx's compile-time checked query macros for
ordinary queries. Those macros require either a build-time database or checked-
in `.sqlx` metadata maintained with `cargo sqlx prepare`. Runtime query APIs plus
real SQLite integration tests keep the build and CI workflow smaller. This can
be reconsidered if SQL changes become a recurring source of defects.

Use `sqlx::migrate!` to embed migrations in the executable. Running the server
must not depend on migration files being present beside the binary.

### Database location and filesystem

The production database is a file on a local filesystem. WAL mode depends on
shared memory and is not suitable for a network filesystem. The database path
will eventually be server configuration; tests will use a unique temporary
file, not a pooled `:memory:` database.

### Connection configuration

Construct connections with `SqliteConnectOptions` rather than a manually
assembled URL. Configure the following explicitly:

- create the database file when it is missing
- enable foreign-key enforcement on every connection
- use WAL journal mode
- use `synchronous=FULL`
- use a five-second busy timeout
- retain the default statement cache initially

Start with a pool maximum of four connections. SQLite allows concurrent readers
but only one writer. A large pool would add worker threads and lock contention,
not write throughput. Make the pool size configurable only when operational
evidence requires it.

`synchronous=FULL` is intentional: in WAL mode it preserves durability across a
power loss. `NORMAL` is faster but can lose recently committed transactions after
a power failure. Reliability is the safer initial default; benchmark before
changing it.

Keep SQLite's default automatic WAL checkpoint behavior initially. Add explicit
checkpoint management only after observing WAL growth or latency in realistic
loads.

### Migrations

Place forward-only migrations in `crates/chat-server/migrations` and run them at
startup before accepting traffic. Startup fails if opening or migrating the
database fails. Never edit an applied migration; add another migration.

The first migration creates these tables:

```sql
CREATE TABLE users (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    display_name  TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0)
) STRICT;

CREATE TABLE conversations (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    title         TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0)
) STRICT;

CREATE TABLE conversation_members (
    conversation_id INTEGER NOT NULL REFERENCES conversations(id),
    user_id         INTEGER NOT NULL REFERENCES users(id),
    role            TEXT NOT NULL CHECK (role IN ('owner', 'member')),
    joined_at_ms    INTEGER NOT NULL CHECK (joined_at_ms >= 0),
    PRIMARY KEY (conversation_id, user_id)
) STRICT;

CREATE TABLE messages (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id INTEGER NOT NULL REFERENCES conversations(id),
    author_id       INTEGER NOT NULL REFERENCES users(id),
    body            TEXT NOT NULL,
    created_at_ms   INTEGER NOT NULL CHECK (created_at_ms >= 0)
) STRICT;

CREATE UNIQUE INDEX conversation_members_one_owner
    ON conversation_members (conversation_id)
    WHERE role = 'owner';

CREATE INDEX conversation_members_by_user
    ON conversation_members (user_id, conversation_id);

CREATE INDEX messages_by_conversation
    ON messages (conversation_id, id DESC);
```

`AUTOINCREMENT` is a deliberate exception to SQLite's general recommendation to
avoid it. These IDs are durable external identities, and message IDs are also
cursors. Reusing the highest deleted ID could make stale client state refer to a
different entity. The extra sequence bookkeeping is acceptable for this
invariant.

The database enforces structural integrity. Text validation remains owned by
`chat`; duplicating all Unicode and whitespace rules in SQL would create two
validation definitions that can drift. Values read from the database are still
reconstructed through the core validated types so corrupt or incompatible data
cannot silently enter the application.

Do not add cascade behavior until deletion semantics are designed. Do not make
messages reference memberships: removing a member must not invalidate their
historical messages.

### Time representation

Store timestamps as non-negative Unix epoch milliseconds in explicitly named
`*_at_ms` INTEGER columns. Milliseconds are sufficient for chat display and map
directly to browser time values. Message ordering and pagination use integer IDs,
not timestamps.

The store assigns `SystemTime::now()`, converts it through one checked helper,
and uses one timestamp for all records created by the same operation. Decoding
also uses a checked helper. Pre-epoch and overflowing values are errors. Do not
add a clock abstraction until a use case needs controllable time.

### Transactions and concurrency

Keep write transactions short and do not perform unrelated async work while a
transaction is open.

| Capability | Transaction |
| --- | --- |
| `CreateUserStore` | one insert; implicit transaction is sufficient |
| `CreateConversationStore` | create conversation and owner atomically |
| `AddMemberStore` | authorize owner, validate target, and insert atomically |
| `RemoveMemberStore` | authorize actor, validate role, and delete atomically |
| `PostMessageStore` | verify membership and insert atomically |
| Read capabilities | one statement each; no explicit transaction initially |

Use an immediate write transaction for multi-statement mutations so lock
acquisition happens at the transaction boundary rather than after authorization
reads. The busy timeout handles short competing writes; exhaustion maps to the
use case's `StoreUnavailable` error.

Authorization and mutation must occur in the same transaction. Never fetch
authorization state, release the connection, and mutate later.

### Query behavior

- Hide missing and unauthorized conversations behind the core's `NotFound`
  errors.
- Use explicit `ORDER BY` clauses for every ordered result.
- List conversations deterministically by descending conversation ID.
- List members deterministically by owner first, then join time and user ID.
- List messages with `conversation_id = ? AND id < ? ORDER BY id DESC`.
- Fetch `limit + 1` messages, return at most `limit`, and set `next_cursor` to
  the last returned ID only when another row exists.

### Error mapping

Do not expose `sqlx::Error` from implementations of `chat` traits.

- expected absence or authorization failure maps to the use-case-specific
  domain error
- an existing membership maps to `AlreadyMember`
- invalid decoded IDs, roles, text, or timestamps map to `InvalidStoreResult`
- pool closure, timeout, I/O, worker failure, lock timeout, and unexpected
  database failures map to `StoreUnavailable`

Prefer explicit existence and authorization queries over parsing SQLite error
messages. SQLx's structured constraint classification may be used as a race-safe
fallback, but error strings must not control application behavior.

Internal errors should retain the original source for tracing. Public core
errors remain intentionally small and transport-independent.

### Module layout

Keep persistence in `chat-server` for now. Do not create another crate before a
second consumer or a real compilation boundary exists.

```text
crates/chat-server/
|-- migrations/
|   `-- 0001_initial.sql
|-- src/
|   |-- lib.rs
|   |-- main.rs
|   |-- sqlite.rs
|   `-- sqlite/
|       |-- read.rs
|       |-- row.rs
|       `-- write.rs
`-- tests/
    `-- sqlite_store.rs
```

`sqlite.rs` owns connection configuration, `SqliteStore`, migrations, common
conversion helpers, and child-module declarations. `main.rs` remains a thin
composition root. Split files further only when their responsibilities become
distinct.

## Verification Plan

Integration tests use Tokio and a unique temporary database file. Each test
opens the store through the same migration and connection path used in
production.

Required coverage:

1. a fresh database migrates and migrations can be run again safely
2. foreign keys and configured journal/synchronous modes are active
3. every core use case succeeds against SQLite
4. every documented domain error is mapped correctly
5. multi-record failures roll back without partial state
6. authorization checks and mutations remain atomic under competing tasks
7. list ordering and message cursor boundaries are correct
8. data remains available after the pool is closed and reopened
9. malformed persisted values are rejected rather than silently accepted
10. `PRAGMA foreign_key_check` reports no violations after the test suite's
    representative workflows

Tests should primarily call `Chat<SqliteStore>` so they verify the real boundary.
Small adapter-level tests are appropriate for migrations, conversion helpers,
and database configuration.

## Implementation Sequence

1. Add minimal workspace dependencies and create the `chat-server` library.
2. Implement connection options, `SqliteStore::open`, embedded migrations, and
   temporary-file test setup.
3. Add the initial STRICT schema and configuration tests.
4. Implement row decoding and checked ID/time/value conversions.
5. Implement and test write capabilities in dependency order: user,
   conversation, membership, message.
6. Implement and test read capabilities and pagination.
7. Add rollback, concurrent mutation, reopen, and integrity tests.
8. Run formatting, Clippy, tests, docs, and a release build.

The milestone is complete only when all nine `chat` storage capability traits
are implemented by one `SqliteStore`, the complete workspace passes CI checks,
and no server transport code is needed to exercise persistence.

## Deferred Work

- HTTP, WebSocket, and authentication
- event fan-out and connection management
- online backup and restore procedures
- explicit WAL checkpoint scheduling
- retention, deletion, and cascade policies
- transactional outbox or durable event delivery
- pool and pragma tuning based on measured production workloads

## Primary References

- [SQLx 0.9 documentation](https://docs.rs/sqlx/0.9.0/sqlx/)
- [SQLx SQLite connection documentation](https://docs.rs/sqlx/0.9.0/sqlx/struct.SqliteConnection.html)
- [SQLx connection options](https://docs.rs/sqlx/0.9.0/sqlx/sqlite/struct.SqliteConnectOptions.html)
- [SQLx migrations](https://docs.rs/sqlx/0.9.0/sqlx/macro.migrate.html)
- [SQLx 0.9 workspace metadata and MSRV](https://github.com/transact-rs/sqlx/blob/v0.9.0/Cargo.toml)
- [Tokio documentation](https://docs.rs/tokio/1.52.3/tokio/)
- [SQLite write-ahead logging](https://www.sqlite.org/wal.html)
- [SQLite transactions](https://www.sqlite.org/lang_transaction.html)
- [SQLite foreign keys](https://www.sqlite.org/foreignkeys.html)
- [SQLite STRICT tables](https://www.sqlite.org/stricttables.html)
- [SQLite AUTOINCREMENT](https://www.sqlite.org/autoinc.html)
- [SQLite data types and time values](https://www.sqlite.org/datatype3.html)
