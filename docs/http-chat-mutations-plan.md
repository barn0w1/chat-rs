# Authenticated HTTP Chat Mutation Plan

Status: implemented for Milestone 4B.2, mechanically verified, and covered by
the production-like E2E verification on 2026-06-22
Date: 2026-06-21
Baseline: `50486b0`

## Decision Summary

Milestone 4B.2 adds the smallest complete authenticated mutation surface:

| Method | Path | Success |
| --- | --- | --- |
| `POST` | `/api/v1/conversations` | `201 Created` with the conversation |
| `POST` | `/api/v1/conversations/{conversation_id}/messages` | `201 Created` with the message |
| `GET` | `/api/v1/conversations/{conversation_id}/messages/{message_id}` | `200 OK` with the message |

The two POST routes require a valid local session, an exact configured Origin,
the session's synchronizer token, and an `application/json` request. The GET
route requires only a valid local session. Every API response remains
`Cache-Control: no-store`.

The individual message read is included because each creation response needs a
stable URI for its newly created resource. It is implemented as an actor-aware
core query, not as a direct HTTP-to-SQL shortcut.

No new production dependency or database migration is required.

## Scope Boundaries

Implement in 4B.2:

- conversation creation
- message posting
- individual message retrieval
- mutation-specific authentication and CSRF extraction
- strict JSON request DTOs and a 64 KiB route-local body limit
- finite request, validation, authorization, and persistence error mappings
- `201 Created`, relative `Location`, and explicit response DTOs
- core, SQLite, and router-level tests for the new behavior

Do not implement in 4B.2:

- membership invitations, addition, removal, or user discovery
- server admission or whitelist policy
- user profile mutation
- conversation renaming or deletion
- message editing or deletion
- idempotency storage or automatic POST retry
- WebSocket publication or an event bus
- CORS or cross-origin browser clients
- test-only production endpoints for manufacturing users or sessions

These are separate product or reliability decisions. In particular,
conversation membership and server admission are distinct policies and must not
be conflated in HTTP handlers.

## HTTP Contract

### Create a conversation

```http
POST /api/v1/conversations
Content-Type: application/json
Cookie: chat_session=...
Origin: https://chat.example.com
X-CSRF-Token: ...

{"title":"General"}
```

The request object has exactly one required string field, `title`. Unknown,
missing, null, and non-string fields are invalid requests. Trimming and domain
validation remain owned by `ConversationTitle` in `chat`.

Success:

```http
HTTP/1.1 201 Created
Location: /api/v1/conversations/42
Cache-Control: no-store
Content-Type: application/json

{
  "id": "42",
  "title": "General",
  "created_at_ms": 1781971200000,
  "role": "owner"
}
```

The relative `Location` avoids coupling the response to an internal bind
address or untrusted forwarded headers. The created resource is the same
conversation returned by `GET /api/v1/conversations/42`. `Content-Location` is
not added in this increment because clients do not require it.

### Post a message

```http
POST /api/v1/conversations/42/messages
Content-Type: application/json
Cookie: chat_session=...
Origin: https://chat.example.com
X-CSRF-Token: ...

{"body":"hello"}
```

The request object has exactly one required string field, `body`. Unknown,
missing, null, and non-string fields are invalid requests. The core preserves a
valid body exactly and applies its existing Unicode-scalar and whitespace
rules.

Success:

```http
HTTP/1.1 201 Created
Location: /api/v1/conversations/42/messages/99
Cache-Control: no-store
Content-Type: application/json

{
  "id": "99",
  "conversation_id": "42",
  "author_id": "7",
  "body": "hello",
  "created_at_ms": 1781971400000
}
```

An absent conversation and a conversation in which the actor is not a member
both produce the same `404` problem. This preserves the existing non-disclosure
rule.

### Get one message

```http
GET /api/v1/conversations/42/messages/99
Cookie: chat_session=...
```

Success returns the same message representation as message creation. A missing
message, a message under another conversation ID, an absent conversation, and
an invisible conversation all produce the same `404` problem.

All wire IDs remain decimal JSON strings. Path IDs remain positive signed
64-bit decimal integers.

## Core Application Change

Creation use cases already have the required semantics:

- `CreateConversation` validates, atomically creates the conversation and owner
  membership, and returns `CreateConversationResult` with events.
- `PostMessage` validates, atomically checks membership and creates the message,
  and returns `PostMessageResult` with events.

Add one query capability:

```text
GetMessageStore::get_message(actor_id, conversation_id, message_id)

Chat::get_message(actor_id, conversation_id, message_id)
    -> Result<Message, GetMessageError>

GetMessageError
|-- NotFound
|-- InvalidStoreResult
`-- StoreUnavailable
```

The use case verifies that the returned message has both requested IDs. Add
`GetMessageStore` to the `ReadStore` capability bundle and export the query and
error from `chat::lib`.

Do not add HTTP, Serde, Origin, CSRF, or `Location` concepts to `chat`.

## SQLite Query

Implement `GetMessageStore` with one actor-aware query:

```sql
SELECT message.id,
       message.conversation_id,
       message.author_id,
       message.body,
       message.created_at_ms
FROM messages AS message
JOIN conversation_members AS viewer
  ON viewer.conversation_id = message.conversation_id
WHERE message.id = ?
  AND message.conversation_id = ?
  AND viewer.user_id = ?
```

`fetch_optional` maps no row to `NotFound`. Persisted-value conversion uses the
existing `MessageRow`. SQL errors map to `StoreUnavailable`; invalid stored
values map to `InvalidStoreResult`.

Conversation creation and message posting continue using their existing
`BEGIN IMMEDIATE` transactions. Authorization and mutation remain within the
same write transaction. No HTTP preflight query is introduced.

## Mutation Authentication and CSRF

Add an `AuthenticatedMutation` extractor in `http/authentication.rs`:

```text
AuthenticatedMutation
`-- user_id: UserId
```

It implements `FromRequestParts<AppState>` and performs, in order:

1. resolve the opaque server-side session cookie
2. require one `Origin` value exactly equal to the origin derived from
   `CHAT_PUBLIC_URL`
3. require `X-CSRF-Token`
4. verify the supplied token against the token stored in the session
5. expose only the local `UserId`

Missing, malformed, or mismatched Origin and CSRF values all return the same
`403` problem. The implementation does not fall back to `Referer` and does not
trust `Host`, `Forwarded`, or `X-Forwarded-*`. This is deliberate: the externally
visible origin is explicit configuration and the deployment may sit behind a
reverse proxy.

Handler extractor order is:

```text
State -> Path (when present) -> AuthenticatedMutation -> Json
```

Axum executes extractors left to right and requires a body-consuming extractor
such as `Json` to be last. Consequently authentication, Origin, CSRF, and path
validation complete before JSON buffering or deserialization.

This retains the existing synchronizer-token plus exact-Origin defense. A
custom header alone can be useful for same-origin APIs, but keeping both checks
is inexpensive and consistent with the established session contract.

## JSON Extraction and Body Limits

Use transport-only request DTOs:

```rust,ignore
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateConversationRequest {
    title: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PostMessageRequest {
    body: String,
}
```

Apply `DefaultBodyLimit::max(64 * 1024)` only to method routers containing the
POST endpoints. Axum's `Json` extractor uses the default body limit and must be
the final handler parameter.

64 KiB is intentionally larger than the worst reasonable JSON encoding of the
4,000-scalar message limit, including escaped Unicode, while still tightly
bounding buffering. The byte limit is a transport resource limit; the core's
character limit remains the product rule.

Accept `Json` as `Result<Json<T>, JsonRejection>` and map by finite category.
Never return Axum's rejection text or Serde's internal error message.

| Failure | Status | Problem suffix |
| --- | --- | --- |
| missing or unsupported JSON `Content-Type` | `415` | `unsupported-media-type` |
| request body over 64 KiB | `413` | `content-too-large` |
| malformed JSON | `400` | `invalid-request` |
| missing, unknown, null, or wrongly typed field | `400` | `invalid-request` |

Since `JsonRejection` is non-exhaustive, mapping uses its stable status category
rather than exhaustively matching internal variants: `413` stays `413`, `415`
stays `415`, and Axum's `400` or `422` JSON syntax/data rejections become the
API's generic `400`. Any other status becomes the finite internal `500`
problem. No branch exposes rejection text.

## Domain Validation Problems

Syntactically valid DTOs that violate core value rules return `422`. Extend the
existing finite field-error representation so `max` is optional:

```json
{
  "type": "urn:chat-rs:problem:validation-failed",
  "title": "Request validation failed",
  "status": 422,
  "errors": [
    { "field": "title", "code": "too_long", "max": 100 }
  ]
}
```

Stable mappings:

| Core error | Field | Code | Optional `max` |
| --- | --- | --- | --- |
| empty conversation title | `title` | `empty` | none |
| title control character | `title` | `contains_control_character` | none |
| long conversation title | `title` | `too_long` | `100` |
| empty message body | `body` | `empty` | none |
| long message body | `body` | `too_long` | `4000` |

Do not include rejected values, actual message lengths, domain `Display` text,
or message content in responses or logs.

## Complete Error Mapping

| Source | Status | Problem suffix |
| --- | --- | --- |
| missing, expired, malformed, or revoked session | `401` | `authentication-required` |
| session user removed between extraction and creation | `401` | `authentication-required` |
| missing or invalid Origin/CSRF | `403` | `forbidden` |
| malformed path ID | `400` | `invalid-request` |
| invalid JSON shape or syntax | `400` | `invalid-request` |
| unsupported JSON media type | `415` | `unsupported-media-type` |
| oversized body | `413` | `content-too-large` |
| title or body domain validation | `422` | `validation-failed` |
| absent or invisible conversation/message | `404` | `not-found` |
| SQLite unavailable | `503` | `service-unavailable` |
| invalid store result, timestamp, header, or representation | `500` | `internal` |

`CreateConversationError::CreatorNotFound` maps to `401`: a session-backed
actor normally exists, so this can only occur when identity continuity was
lost after session extraction. `PostMessageError::ConversationNotFound` and
`AuthorNotMember` both map to `404`.

## Response Construction

Extend `http/representation.rs` rather than adding Serde derives to domain
types:

- convert `CreateConversationResult` to the existing conversation DTO with
  role `owner`
- convert `PostMessageResult::message()` to the existing message DTO
- convert `GetMessage` results to the same message DTO
- construct `201` responses with `Location` and `Cache-Control: no-store`

Location values contain only fixed ASCII route segments and validated positive
integer IDs. Header conversion failure still maps to `500` rather than using
`unwrap`.

The response body is the state returned by the committed core mutation. Do not
query the database again merely to build the response.

## Events and Retry Semantics

Core mutation results continue returning `ChatEvent` values. The HTTP handler
uses the stored entity for its response and intentionally drops the event list
because no event consumer exists yet. Do not introduce a no-op publisher or an
in-memory channel in this phase. Milestone 5 will add publication when a real
WebSocket consumer and backpressure policy exist.

Both POST operations are non-idempotent. A client that loses the response
cannot know whether the commit occurred and must not automatically retry. The
expired `Idempotency-Key` Internet-Draft is not an RFC, and adopting its header
alone would not solve atomic deduplication.

Before automatic retry is enabled, separately design:

- a client-generated operation identifier
- actor and operation scoping
- payload fingerprint conflict behavior
- an SQLite uniqueness constraint
- atomic operation record and mutation commit
- replay of the original success representation
- retention and cleanup policy
- interaction with future event publication

## Module Changes

Expected files:

```text
crates/chat/src/
|-- get_message.rs                         # new query use case
|-- lib.rs                                 # export query/error
`-- store.rs                               # GetMessageStore and ReadStore

crates/chat-server/src/
|-- http/authentication.rs                 # AuthenticatedMutation
|-- http/conversation.rs                   # POST and item GET handlers
|-- http/problem.rs                        # finite media/body/field problems
|-- http/representation.rs                 # 201 and item representations
|-- sqlite/read.rs                         # actor-aware message lookup
`-- ...tests                               # focused additions
```

Keep the current module tree. `conversation.rs` remains the owner of these
closely related routes; do not add a generic controller, service layer, or
request-validation framework.

## Test Strategy

### Core tests

- `GetMessage` returns a matching message
- wrong message or conversation ID is rejected as `InvalidStoreResult`
- `NotFound` and `StoreUnavailable` propagate unchanged
- existing creation and posting validation remains unchanged

### SQLite integration tests

- a member can fetch a message by both IDs
- an outsider receives `NotFound`
- the wrong conversation ID receives `NotFound`
- missing messages receive `NotFound`
- posted content survives reopening and can be fetched individually

### Router tests

Use the real Axum router, real temporary SQLite, and Tower `oneshot`:

- valid conversation creation returns exact `201`, `Location`, DTO, and
  `no-store`
- valid message posting returns exact `201`, `Location`, DTO, and `no-store`
- created message URI returns the same representation
- actor IDs always come from the session
- missing session returns `401` before CSRF or JSON errors
- missing, wrong, and malformed Origin/CSRF return `403`
- a malformed or oversized body without valid CSRF still returns `403`, proving
  extractor ordering
- unsupported media type returns `415`
- body over 64 KiB returns `413`
- invalid JSON and unknown/missing/wrongly typed fields return `400`
- each domain validation variant returns the exact finite `422` field error
- outsider message posting and retrieval return `404`
- malformed and mismatched path IDs return finite `400` or `404` as specified
- a rejected request produces no conversation or message
- closed SQLite produces `503` without internal details
- existing GET, HEAD, `404`, `405`, and `Allow` behavior remains intact

Do not start a TCP listener for these protocol-contract tests. `Router::oneshot`
exercises routing, extraction, authentication, handlers, DTOs, and real SQLite
without adding scheduling and port-allocation noise. Existing server tests
already cover bind and graceful shutdown. A later operational smoke test can
exercise the complete process behind a reverse proxy and real or controlled
OIDC provider; no test-only HTTP authentication bypass is added.

## Implementation Slices

### Slice 1: Individual-message core query

- add `GetMessage`, `GetMessageError`, and `GetMessageStore`
- validate store results
- update `ReadStore` and core contract tests

Completion: a transport-independent actor can retrieve one message safely.

### Slice 2: SQLite lookup

- implement the actor-aware single query
- reuse persisted row conversion
- add visibility and mismatch integration tests

Completion: absent and invisible messages are indistinguishable.

### Slice 3: Mutation request boundary

- add `AuthenticatedMutation`
- add strict DTOs and JSON rejection mapping
- add `413` and `415` finite problems
- apply the route-local 64 KiB limit

Completion: unsafe request prerequisites are checked before body extraction.

### Slice 4: Handlers and representations

- add the two POST routes and one GET route
- map core errors exhaustively
- add `201` response and relative `Location` construction
- reuse explicit conversation and message representations

Completion: authenticated clients can create and retrieve the initial mutable
resources without exposing domain or storage internals.

### Slice 5: Verification and documentation

- add core, SQLite, and router tests
- run formatting, compilation, locked Clippy, workspace tests, and release
  build on Rust 1.96
- update README status after mechanical verification
- cover the real-provider, reverse-proxy, and browser path in the
  production-like E2E verification

Completion: 4B.2 is independently reviewable and mergeable before reliability
or real-time work.

## Acceptance Criteria

- `chat` remains independent of HTTP, Axum, Serde, cookies, and CSRF
- no new production dependency or schema migration is introduced
- both mutations derive the actor exclusively from the verified session
- Origin and CSRF are checked before JSON body extraction
- mutation authorization and writes remain atomic in SQLite
- individual message retrieval is actor-aware in one query
- every created resource has a stable relative `Location`
- HTTP DTOs use string IDs and integer millisecond timestamps
- malformed input and all domain errors have finite non-leaking mappings
- invisible and absent conversation-scoped resources remain indistinguishable
- request bodies are bounded to 64 KiB on mutation routes
- handlers do not log cookies, CSRF values, titles, or message bodies
- mutation events are preserved by the core without premature publication
- POST retry and membership/admission policy remain explicitly out of scope
- existing behavior and tests continue to pass
- formatting, compilation, locked Clippy, workspace tests, and release build
  pass on Rust 1.96

## Primary References

- [RFC 9110: HTTP Semantics](https://www.rfc-editor.org/rfc/rfc9110.html)
- [RFC 9457: Problem Details for HTTP APIs](https://www.rfc-editor.org/rfc/rfc9457.html)
- [Axum 0.8.9 extractors](https://docs.rs/axum/0.8.9/axum/extract/index.html)
- [Axum 0.8.9 `Json`](https://docs.rs/axum/0.8.9/axum/struct.Json.html)
- [Axum 0.8.9 `JsonRejection`](https://docs.rs/axum/0.8.9/axum/extract/rejection/enum.JsonRejection.html)
- [Axum 0.8.9 `DefaultBodyLimit`](https://docs.rs/axum/0.8.9/axum/extract/struct.DefaultBodyLimit.html)
- [Serde container attributes](https://serde.rs/container-attrs.html)
- [OWASP CSRF Prevention](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html)
- [Expired Idempotency-Key draft](https://datatracker.ietf.org/doc/draft-ietf-httpapi-idempotency-key-header/)
- [The Rust Book: Test Organization](https://doc.rust-lang.org/book/ch11-03-test-organization.html)
