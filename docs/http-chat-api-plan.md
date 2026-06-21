# Authenticated HTTP Chat API Plan

Status: Milestones 4B.1 and 4B.2 implemented
Date: 2026-06-21

## Decision Summary

Milestone 4B is divided into reviewable increments.

- **4B.1, implemented:** refactor the HTTP boundary into focused modules, add
  pagination contracts exposed by the first real transport, and implement
  authenticated read endpoints for conversations, members, and messages.
- **4B.2, implemented:** add conversation creation and message posting with
  JSON body extraction, CSRF and Origin enforcement, creation responses, and
  individual message retrieval.
- **4B.3 or a later reliability increment:** add atomic operation
  deduplication before the web client automatically retries mutations.
- **Membership workflow, separately planned:** design invitations, acceptance,
  user discovery policy, and removal together before exposing membership
  mutations.

Increment 4B.1 implements the security and representation boundary as an
independently reviewable change. It does not change core mutation contracts,
deduplication persistence, or future event delivery.

## Current Constraints

The existing system already provides:

- a transport-independent `chat` crate
- actor-aware read and write use cases
- SQLite implementations that perform authorization in the same query or
  transaction as data access
- opaque server-side browser sessions
- a request extractor that derives the actor from the session cookie
- exact Origin and synchronizer-token checks for the existing unsafe endpoint
- RFC 9457-shaped problem responses

The HTTP adapter exposes core behavior; it does not reimplement membership or
authorization rules. The authenticated actor always comes from the session.
No request path, query, or body may override it.

The first transport also exposes two core limitations that should be corrected
while interfaces are still unstable:

- `list_conversations` returns an unbounded collection.
- `list_members` returns an unbounded collection.

Both become paginated before they are made HTTP resources. Message history is
already cursor-paginated.

## Scope of 4B.1

Implement these routes under `/api/v1`:

| Method | Path | Result |
| --- | --- | --- |
| `GET` | `/conversations?before=&limit=` | visible conversation page |
| `GET` | `/conversations/{conversation_id}` | one visible conversation |
| `GET` | `/conversations/{conversation_id}/members?after=&limit=` | visible member page |
| `GET` | `/conversations/{conversation_id}/messages?before=&limit=` | visible message page |

All four routes require a valid local session. Every response includes
`Cache-Control: no-store`.

Do not implement in 4B.1:

- user registration or profile endpoints
- conversation or message mutation endpoints
- add-member or remove-member endpoints
- user directory or user search
- invitations
- WebSocket delivery
- event publication
- generic plugin registries
- mutation idempotency storage
- CORS

The identity adapter produces `VerifiedIdentity`; server admission then decides
whether a first login may provision the local user. A separate public
`POST /users` would duplicate that policy and is not part of the browser
protocol.

## Pagination Changes in `chat`

### Conversations

Replace the unbounded query with a value shaped like the existing message
query:

```text
ListConversations
|-- before: Option<ConversationId>
`-- limit: usize

ConversationPage
|-- conversations: Vec<ConversationSummary>
`-- next_cursor: Option<ConversationId>
```

Use:

- default limit: `50`
- maximum limit: `100`
- exclusive `before` cursor
- descending conversation ID order

The SQLite query fetches `limit + 1`, truncates to `limit`, and returns the last
included ID as `next_cursor` only when another row exists. The core validates
page size, uniqueness, descending IDs, cursor bounds, and cursor consistency.

This changes `ListConversationsStore` and its tests. It is an intentional early
breaking change driven by the first concrete transport.

### Members

Introduce:

```text
ListMembers
|-- conversation_id: ConversationId
|-- after: Option<UserId>
`-- limit: usize

MemberPage
|-- members: Vec<ConversationMember>
`-- next_cursor: Option<UserId>
```

Use:

- default limit: `100`
- maximum limit: `200`
- exclusive `after` cursor
- ascending user ID order

Members are not a time-ordered feed, so ascending user ID is a simpler stable
ordering than a composite role and join-time cursor. Role remains explicit in
each representation; clients must not infer authority from position.

The SQLite query must first establish that the actor can view the conversation,
then page members within that conversation. The core validates page size,
unique users, ascending IDs, cursor bounds, conversation consistency, and
cursor consistency.

### Messages

Keep the existing contract:

- default limit: `50`
- maximum limit: `100`
- exclusive `before: MessageId`
- newest-first order
- `next_cursor` is the last returned message ID only when older data exists

No opaque encoded cursor is needed yet. Integer IDs are immutable and ordering
is explicit. All wire IDs remain decimal JSON strings to preserve exact values
in JavaScript.

## HTTP Representations

Do not derive Serde traits on `chat` domain types. Define transport DTOs in
`chat-server` and convert explicitly.

### Conversation page

```json
{
  "conversations": [
    {
      "id": "42",
      "title": "General",
      "created_at_ms": 1781971200000,
      "role": "owner"
    }
  ],
  "next_cursor": "42"
}
```

`next_cursor` is `null` when there is no next page.

The single-conversation endpoint returns one conversation object with the same
fields, not an additional envelope.

### Member page

```json
{
  "members": [
    {
      "user": {
        "id": "7",
        "display_name": "Yuito",
        "created_at_ms": 1781971200000
      },
      "role": "member",
      "joined_at_ms": 1781971300000
    }
  ],
  "next_cursor": null
}
```

### Message page

```json
{
  "messages": [
    {
      "id": "99",
      "conversation_id": "42",
      "author_id": "7",
      "body": "hello",
      "created_at_ms": 1781971400000
    }
  ],
  "next_cursor": "99"
}
```

Including `conversation_id` in the message representation allows the same DTO
to be reused by later creation responses and live events without depending on
the containing URL.

## Input Contract

- Path IDs are positive base-10 integers within SQLite's signed 64-bit range.
- Query cursors use the same decimal format.
- `limit` is a decimal integer and is optional.
- Missing limits use the core defaults.
- Unknown query fields are rejected.
- Zero, negative, overflowing, malformed, and empty IDs are `400` invalid
  requests.
- Structurally valid limits outside the documented range are `422` validation
  failures.

Deserialize query DTOs with `#[serde(deny_unknown_fields)]`. Convert strings to
the existing strong ID types after extraction so conversion failures are
mapped to the stable HTTP problem contract rather than Axum's default text
response.

## Authentication and Authorization

Refine the request-derived value to expose only the local actor ID to chat
handlers:

```text
AuthenticatedUser
`-- user_id: UserId
```

The session resource may continue using the full authenticated session because
it must return the CSRF value. Chat handlers receive only `AuthenticatedUser`.

Every resource lookup calls the actor-aware `chat` use case. Handlers must not
query `SqliteStore` directly to preflight authorization. This preserves atomic
authorization behavior and prevents an IDOR check from being omitted on a new
route.

For conversation-scoped reads, both an absent resource and a resource invisible
to the actor return the same `404` problem. This matches the existing core
contract and avoids disclosing conversation existence.

The existing `401` response should gain a documented `WWW-Authenticate`
challenge. Use a private `ChatSession` scheme for this application:

```text
WWW-Authenticate: ChatSession realm="chat"
```

This is an application protocol marker, not a claim of a standardized reusable
authentication scheme. It tells clients that the resource requires the local
session established by the configured login flow. Authentication responses
remain `no-store`.

## Problem Mapping

All API failures use `application/problem+json`. The HTTP status and the
serialized `status` member must match. Clients branch on the stable `type` URN,
never on `title` or internal text.

| Source | Status | Type suffix |
| --- | --- | --- |
| missing, malformed, expired, or revoked session | `401` | `authentication-required` |
| malformed path ID or query syntax | `400` | `invalid-request` |
| page size outside the accepted range | `422` | `validation-failed` |
| absent or invisible conversation | `404` | `not-found` |
| SQLite temporarily unavailable | `503` | `service-unavailable` |
| invalid core/store result or timestamp conversion | `500` | `internal` |

Validation problems may add a finite extension:

```json
{
  "type": "urn:chat-rs:problem:validation-failed",
  "title": "Request validation failed",
  "status": 422,
  "errors": [
    { "field": "limit", "code": "out_of_range", "max": 100 }
  ]
}
```

Do not serialize Rust `Display` output, Axum rejection body text, SQL errors,
paths, query contents, or backtraces. Log internal failures at the boundary
without logging cookies or future message bodies.

## HTTP Module Boundaries

`app.rs` is already becoming too broad. Split by ownership without introducing
a framework abstraction:

```text
crates/chat-server/src/
|-- app.rs                    # AppState and top-level router composition
|-- http.rs                   # shared HTTP module declarations
|-- http/
|   |-- authentication.rs     # authenticated request extractors
|   |-- conversation.rs       # 4B conversation/member/message routes
|   |-- problem.rs            # finite RFC 9457 mappings
|   |-- representation.rs     # explicit response DTO conversion
|   `-- session.rs            # existing session handlers
`-- ...
```

Use the modern `http.rs` plus `http/*.rs` layout, not `http/mod.rs`.

`AppState` should own both the readiness store and the core application entry
point:

```text
AppState
|-- store: SqliteStore
|-- chat: Chat<SqliteStore>
|-- auth: AuthStore
|-- cookies: CookiePolicy
|-- expected_origin
`-- oidc
```

Make `Chat<S>` cloneable when `S: Clone`; cloning `SqliteStore` clones the pool
handle, not the database. Route modules return `Router<AppState>` and are
combined by `merge` or `nest` before one final `with_state` call. This follows
Axum's stateful-router composition model and leaves features in navigable
compile-time modules.

Do not add an application service trait, repository trait aliases, dynamic
handler registry, or dependency injection container. Existing core capability
traits and Axum's typed state are sufficient.

## Extensibility Position

The Minecraft-server analogy is useful at the level of clear capabilities and
independent features, but a dynamic plugin system is premature.

Preserve these extension boundaries:

- authentication methods produce `VerifiedIdentity`
- HTTP modules translate protocol values to core use cases
- core mutations return `ChatEvent` values after persistence
- persistence implements small core capability traits
- the composition root chooses concrete adapters

Do not promise a stable Rust plugin ABI. Rust does not provide a general stable
ABI for arbitrary crate types. If third-party runtime extensions become a real
requirement, evaluate a process protocol or a constrained WASM guest interface
from explicit capability and trust requirements. Internal compile-time modules
are sufficient now.

Do not introduce an event publisher in 4B.1 because there is no consumer. In
4B.2, keep mutation results intact; Milestone 5 can add a concrete event hub at
the composition boundary when WebSocket delivery supplies the first consumer.

## Deferred Mutation Contract

4B.2 is specified in
[`http-chat-mutations-plan.md`](http-chat-mutations-plan.md) and implemented as
the mutation increment following 4B.1:

| Method | Path | Success |
| --- | --- | --- |
| `POST` | `/conversations` | `201`, `Location`, conversation representation |
| `POST` | `/conversations/{conversation_id}/messages` | `201`, `Location`, message representation |
| `GET` | `/conversations/{conversation_id}/messages/{message_id}` | `200`, message representation |

The individual-message read use case is added with message creation so each
`201` response can identify a retrievable resource through `Location`.

Mutation request DTOs reject unknown fields and use a route-local 64 KiB
`DefaultBodyLimit`. This leaves room for 4,000 Unicode scalar values represented
as UTF-8 or JSON escapes while remaining small enough to bound buffering. The
core character limit remains authoritative. Axum `Json` is the final extractor
because it consumes the body. A shared authenticated-mutation extractor
validates the session, exact configured Origin, and `X-CSRF-Token` before the
body is accepted.

The current IETF `Idempotency-Key` document expired on 2026-04-18 and is not an
RFC. 4B.2 therefore does not claim generic idempotency. POST endpoints are
documented as non-idempotent, and the first web client must not automatically
retry an ambiguous POST. Before automatic retries are added, design a
transport-independent operation ID and persist deduplication in the same SQLite
transaction as creation, including payload-conflict and event-replay behavior.

## Deferred Membership Workflow

Do not expose `AddMember` or `RemoveMember` directly in 4B.

The core currently adds a known `UserId`, but the product has no safe way to
discover a user, no unique public handle, no invitation acceptance, and no
privacy policy for a global directory. A raw user-ID endpoint would couple a
guessable database identifier to an incomplete product flow.

Plan membership as a complete capability:

1. owner creates a bounded, expiring invitation
2. an authenticated target explicitly accepts it
3. acceptance atomically creates membership
4. owner removal and voluntary leave become reachable
5. owner transfer or conversation deletion resolves the current
   `OwnerCannotLeave` limitation

This may require new core use cases and schema. It should not be hidden inside
HTTP handlers.

## Test Strategy

Follow Rust's unit/integration distinction without duplicating every assertion.

### Core unit and contract tests

- default, zero, maximum, and over-maximum page sizes
- exclusive cursor behavior
- stable ordering and duplicate rejection
- cursor/result mismatch rejection
- wrong conversation/member/message association rejection
- stores are not called for invalid page sizes

### SQLite integration tests

- multiple pages without gaps or duplicates
- empty visible collections
- invisible conversations return `NotFound`
- concurrent inserts between page requests do not duplicate already-consumed
  descending pages
- member pagination remains stable across role values

### HTTP router tests

Use a temporary real SQLite database and Tower `oneshot`:

- missing session returns the exact `401` problem and challenge
- actor ID always comes from the session
- another user's numeric conversation ID returns the same `404` as an absent ID
- malformed and overflowing IDs return `400`
- invalid limits and unknown query fields return finite problems
- every ID is a JSON string, including values above JavaScript's exact integer
  range in conversion tests
- timestamps and role names are exact
- empty pages and `null` cursors are exact
- all authenticated responses and problems are `no-store`
- closed SQLite produces `503` without internal details
- existing health and session behavior remains unchanged
- standard `404`, `405`, `Allow`, and `HEAD` behavior is preserved

Unit-test pure DTO conversion and problem mapping inside their modules. Keep
end-to-end route contracts in `chat-server` tests when they only need public
server construction; otherwise keep focused private-router tests beside the
module. Do not add snapshot or mocking dependencies.

## Implementation Slices for 4B.1

### Slice 1: Core pagination

- add conversation and member query/page types
- update store capabilities and use-case validation
- update in-memory contract tests

Completion: unbounded conversation and member reads no longer exist in the
core API.

### Slice 2: SQLite pagination

- update read queries to fetch one extra row
- preserve actor-aware visibility checks
- add real-database page and authorization tests

Completion: SQLite satisfies the new core page contracts under empty, normal,
and unauthorized cases.

### Slice 3: HTTP foundation refactor

- split `app.rs` by responsibility
- centralize RFC 9457 problem conversion
- add `AuthenticatedUser`
- add strict path/query conversion
- add the `ChatSession` challenge and `no-store` policy

Completion: existing health and session routes behave identically except for
the documented authentication challenge.

### Slice 4: Read routes and representations

- add the four 4B.1 routes
- add explicit conversation, user, member, and message DTOs
- map every core error exhaustively
- add router-level protocol and authorization tests

Completion: an authenticated browser can load its conversation list, one
conversation, members, and paginated history without any mutation endpoint.

### Slice 5: Verification and documentation

- run formatting, compilation, locked Clippy, tests, docs, and release build
- verify pagination with `curl` using a test-created session fixture or browser
- update README status and endpoint documentation

Completion: 4B.1 is independently mergeable before mutation semantics are
introduced.

## Acceptance Criteria for 4B.1

- `chat` remains independent of Axum, Serde, cookies, and HTTP
- conversation and member collections are cursor-paginated in core and SQLite
- all HTTP actor identity comes from a valid session
- every object lookup is authorized through an actor-aware core use case
- invisible and absent conversation resources are indistinguishable
- wire IDs are decimal strings and timestamps are integer milliseconds
- all extractor and domain failures use finite RFC 9457 mappings
- authenticated responses are not cacheable
- no request or response exposes a raw session, external identity, SQL error,
  or internal invariant text
- no new production dependency is required
- current health, OIDC, session, SQLite, and core tests continue to pass
- formatting, compilation, locked Clippy, workspace tests, documentation, and
  release build pass on Rust 1.96

## Primary References

- [RFC 9110: HTTP Semantics](https://www.rfc-editor.org/rfc/rfc9110.html)
- [RFC 9457: Problem Details for HTTP APIs](https://www.rfc-editor.org/rfc/rfc9457.html)
- [Axum 0.8.9 `Router`](https://docs.rs/axum/0.8.9/axum/struct.Router.html)
- [Axum 0.8.9 `State`](https://docs.rs/axum/0.8.9/axum/extract/struct.State.html)
- [Axum 0.8.9 `Json`](https://docs.rs/axum/0.8.9/axum/struct.Json.html)
- [Axum 0.8.9 `DefaultBodyLimit`](https://docs.rs/axum/0.8.9/axum/extract/struct.DefaultBodyLimit.html)
- [Axum rejection types](https://docs.rs/axum/0.8.9/axum/extract/rejection/index.html)
- [Serde container attributes](https://serde.rs/container-attrs.html)
- [OWASP IDOR Prevention](https://cheatsheetseries.owasp.org/cheatsheets/Insecure_Direct_Object_Reference_Prevention_Cheat_Sheet.html)
- [OWASP CSRF Prevention](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html)
- [Expired Idempotency-Key draft](https://datatracker.ietf.org/doc/draft-ietf-httpapi-idempotency-key-header/)
- [The Rust Book: Test Organization](https://doc.rust-lang.org/book/ch11-03-test-organization.html)
