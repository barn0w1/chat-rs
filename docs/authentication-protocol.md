# Authentication and Protocol Plan

Status: Milestone 4A implemented; local Rust verification pending
Date: 2026-06-21

## Scope Decision

Milestone 4 is split into two reviewable increments.

- **4A, next implementation:** verified identity boundary, OIDC adapter,
  account resolution, server-side sessions, cookie and CSRF handling, and the
  versioned HTTP protocol foundation.
- **4B, subsequent implementation:** authenticated HTTP routes that translate
  chat commands and queries into the existing `chat` use cases.

This document fixes the implementation contract for 4A. It records the 4B wire
conventions, but 4B should receive a separate endpoint-by-endpoint review after
4A is verified. WebSocket connection management remains Milestone 5.

This split is intentional. Implementing external identity verification,
sessions, every chat route, and WebSocket delivery together would make security
and protocol failures difficult to isolate.

## Terminology and Boundaries

Use four distinct concepts:

- **Credential evidence:** an OIDC authorization code and ID token, a future
  passkey assertion, a client certificate, or another method-specific proof.
- **Verified identity:** a stable identifier produced only after an adapter has
  validated that evidence.
- **User:** the application actor represented by `chat::User` and `UserId`.
- **Session:** a revocable, time-bounded continuity mechanism that maps a
  browser request to one `UserId`.

Authentication answers which application user is acting. Authorization remains
inside the chat use cases and stores, where the authenticated `UserId` is
checked against conversation membership and role.

The `chat` crate must not learn about HTTP, cookies, OIDC, JWTs, browser origins,
or session tokens. These concerns belong to `chat-server`.

## Core Design

Each authentication method owns its transport and cryptographic verification.
After successful verification it produces one small internal value:

```text
VerifiedIdentity
|-- authority
|-- subject
`-- profile_hint
```

`authority` and `subject` are opaque, case-sensitive strings forming a namespaced
identity key. For OIDC, the adapter maps the validated `iss` claim to authority
and `sub` to subject. A future method defines its own stable authority namespace
without changing session or chat code. OpenID Connect only guarantees stability
and uniqueness for the combination of `iss` and `sub`; email and profile claims
can change or be reassigned.

Do not define a broad `Authenticator` trait before a second method exists. The
stable integration boundary is the concrete `VerifiedIdentity` value and the
identity-resolution service that consumes it. A future authentication adapter
can produce the same value without implementing OIDC-shaped behavior.

Handlers and chat use cases receive only:

```text
AuthenticatedUser
`-- user_id: UserId
```

They never receive an ID token, external subject, raw session token, or cookie.
An actor ID supplied in JSON is never trusted; the actor always comes from the
validated session extractor.

## First Authentication Adapter

Implement standards-based OpenID Connect Authorization Code Flow as the first
human-user adapter. It is configured by issuer and client metadata rather than
hard-coded for Google. Google and other conforming providers can then be used
without changing the session or chat layers.

Use:

- provider discovery
- authorization code flow, never implicit flow
- PKCE with `S256`, including for a confidential server client
- transaction-specific `state` and `nonce`
- exact configured redirect URI
- ID token signature, issuer, audience, expiry, and nonce validation
- only the validated `iss` and `sub` pair as the external identity key

The OIDC adapter may read `name` as an initial display-name hint. It must not use
email, preferred username, or any profile claim as an identity key.

Do not store provider access tokens or refresh tokens. The chat server needs
authentication, not ongoing access to provider APIs. Avoiding those tokens
reduces both privilege and secret lifecycle work.

### OIDC HTTP client

Use the asynchronous `openidconnect` API with Rustls. Build its HTTP client with
redirect following disabled, as required by the crate's SSRF guidance. Apply
bounded connect and request timeouts.

Perform initial discovery before the server reports ready when OIDC is enabled.
Cache provider metadata and keys. Refresh metadata on a bounded interval and
once on a key/signature validation failure before rejecting the login. Serialize
forced refreshes so invalid tokens cannot create unbounded discovery traffic.

A provider outage after startup must prevent new logins but must not invalidate
existing local sessions or make SQLite readiness fail.

## OIDC Browser Flow

### Start

`GET /auth/oidc/start` performs the following:

1. Generate independent random `state`, nonce, PKCE verifier, and browser-binding
   values.
2. Store a short-lived login transaction in SQLite. Store hashes for values
   used only for lookup or comparison.
3. Set a short-lived, HttpOnly browser-binding cookie.
4. Redirect with `303 See Other` to the provider authorization endpoint.

Do not accept an arbitrary post-login redirect URL. Successful authentication
returns to `/`. A path-only return target can be added later with an explicit
allowlist if the web client requires it.

### Callback

`GET /auth/oidc/callback` performs the following:

1. Require both returned `state` and the browser-binding cookie.
2. Atomically consume one unexpired login transaction.
3. Exchange the authorization code using its stored PKCE verifier.
4. Validate the ID token and nonce through `openidconnect`.
5. Produce `VerifiedIdentity { authority, subject, profile_hint }`.
6. Resolve or provision the local `User` in one SQLite transaction.
7. Revoke any session token presented by this browser and issue a new session.
8. Set the session cookie, clear the login cookie, and redirect to `/`.

The transaction is single-use even when the provider exchange fails. The user
starts a fresh login rather than reusing security material.

Callback failures expose no token, claim, SQL, or cryptographic details to the
browser. Detailed causes are logged without logging codes, raw tokens, session
tokens, state, nonce, PKCE verifier, client secret, or cookies.

## Account Resolution and Provisioning

Identity verification and account provisioning are different operations.
The OIDC adapter verifies identity; a server-owned identity service decides
which `UserId` that identity represents.

For the first implementation:

- An existing `(authority, subject)` binding returns its current user.
- A new binding atomically creates a user and the binding.
- Concurrent first logins for the same identity must create exactly one user.
- A valid provider `name` claim is passed through `chat::DisplayName` validation.
- Missing or invalid profile names use the neutral value `New user`.
- Later profile-claim changes do not silently rename the chat user.

This orchestration belongs to `chat-server` because the transaction spans an
external identity binding and a core user. Reuse core validation types and
factor the SQLite user insert so the existing `CreateUserStore` and provisioning
path do not duplicate SQL behavior. Do not add OIDC concepts to `chat` merely to
make the transaction look uniform.

Account linking, unlinking, merging, provider migration, and an onboarding name
screen are non-goals for 4A. They require explicit user-presence and recovery
policies and must not emerge as side effects of login.

## Server-Side Sessions

Use opaque server-side sessions rather than using an ID token or self-contained
JWT as the application session.

For every session:

- generate a 32-byte token from the operating system CSPRNG
- encode it as unpadded URL-safe Base64 for the cookie
- store only `SHA-256(token)` in SQLite
- generate and store a separate 32-byte CSRF token
- bind the session to one `UserId`
- use a fixed absolute lifetime, initially 30 days
- allow multiple sessions for multiple browsers/devices
- reject expired or missing sessions as unauthenticated
- revoke the current session on logout

The raw session token is a meaningless bearer secret. Hashing it before
persistence prevents the database value itself from being usable as the
cookie. The CSRF value is stored in recoverable form because the session
resource must return the same synchronizer token to same-origin JavaScript.
It is not sufficient to authenticate a request without the HttpOnly session
cookie. A random-token generation failure is an operation failure; never fall
back to predictable data.

Do not add idle expiry or write `last_seen` on every request in 4A. That would
turn reads and WebSocket traffic into continuous SQLite writes. Absolute expiry
and explicit revocation are sufficient for the first implementation.

Expired session and OIDC transaction rows are deleted opportunistically during
creation and authentication. A dedicated maintenance task is unnecessary at
this scale.

## Cookie Contract

Derive cookie security from a validated `CHAT_PUBLIC_URL`, never from forwarded
or request `Host` headers.

For HTTPS deployments:

```text
name: __Host-chat_session
Path: /
Domain: absent
Secure: true
HttpOnly: true
SameSite: Lax
Max-Age: session lifetime
```

The `__Host-` prefix requires a host-only Secure cookie with `Path=/`. For local
development over plain HTTP, allow only a loopback public URL and use
`chat_session` without `Secure`. Reject a non-loopback `http` public URL at
startup.

Use a separate short-lived login-binding cookie with the same host, path,
HttpOnly, Secure, and SameSite policy. Clear cookies with the same name and
attributes used to set them.

`SameSite=Lax` is defense in depth, not the only CSRF control. It also permits
the top-level navigation back from an OIDC provider.

## CSRF and Origin Policy

Cookie authentication is ambient authority, so state-changing HTTP requests
must require a synchronizer CSRF token.

- `GET /api/v1/session` returns the raw CSRF token to same-origin JavaScript.
- The web client keeps it in memory and sends it as `X-CSRF-Token` on every
  unsafe API request.
- The server decodes it and compares the fixed-size value with the session's
  stored value using best-effort constant-time equality.
- The token never appears in a URL, cookie, or log.
- Login creates a new session and therefore rotates the CSRF token.

Also require the request `Origin` to exactly match `CHAT_PUBLIC_URL` for unsafe
cookie-authenticated API requests. Reject absent or mismatched Origin for the
browser session mechanism. SameSite and JSON-only content types remain further
defenses. Do not enable CORS in 4A.

OIDC callback uses its state, nonce, PKCE, and browser-binding checks rather
than the API CSRF header.

Future WebSocket upgrades must validate both the session cookie and exact
Origin before returning `101`. Browser WebSocket authentication uses the same
cookie; tokens must not be placed in query strings or subprotocol values.

## Configuration Contract

Add:

| Variable | Required | Meaning |
| --- | --- | --- |
| `CHAT_PUBLIC_URL` | no | externally visible origin; defaults to `http://127.0.0.1:3000` |
| `CHAT_OIDC_ISSUER` | paired | OIDC issuer URL |
| `CHAT_OIDC_CLIENT_ID` | paired | registered client ID |
| `CHAT_OIDC_CLIENT_SECRET` | no | confidential-client secret when required |

`CHAT_PUBLIC_URL` must contain only `http` or `https`, host, and optional port.
Reject user info, query, fragment, and non-root path. Derive the callback URL as
`/auth/oidc/callback` from this trusted value.

OIDC is disabled when issuer and client ID are both absent. If exactly one is
present, startup fails. When enabled, discovery or invalid provider metadata is
a startup failure. Never include the client secret in `Debug`, `Display`, or
tracing output; configuration types containing it need a redacted/manual Debug
implementation.

The listen address and public URL are deliberately separate. This supports a
loopback server behind an HTTPS reverse proxy without trusting forwarded
headers.

## Persistence Schema

Add an embedded `0002_authentication.sql` migration.

```text
auth_identities
|-- authority TEXT
|-- subject TEXT
|-- user_id INTEGER -> users.id
|-- created_at_ms INTEGER
`-- UNIQUE (authority, subject)

auth_sessions
|-- token_hash BLOB PRIMARY KEY, exactly 32 bytes
|-- csrf_token BLOB, exactly 32 bytes
|-- user_id INTEGER -> users.id
|-- created_at_ms INTEGER
`-- expires_at_ms INTEGER

oidc_login_transactions
|-- state_hash BLOB PRIMARY KEY, exactly 32 bytes
|-- browser_binding_hash BLOB, exactly 32 bytes
|-- nonce TEXT
|-- pkce_verifier TEXT
|-- created_at_ms INTEGER
`-- expires_at_ms INTEGER
```

Add indexes on `auth_identities(user_id)`, `auth_sessions(user_id)`,
`auth_sessions(expires_at_ms)`, and `oidc_login_transactions(expires_at_ms)`.
Use strict tables, foreign keys, nonnegative timestamp checks, and
`expires_at_ms > created_at_ms` checks.

Do not persist raw session, state, or browser-binding tokens. The CSRF token,
nonce, and PKCE verifier must be recoverable for their protocol steps. They are
not authentication bearer credentials by themselves.

## Versioned HTTP Protocol

Application API routes live below `/api/v1`. Authentication redirect routes
remain below `/auth/oidc` because they are browser protocol endpoints rather
than JSON resources.

4A exposes:

| Method | Path | Result |
| --- | --- | --- |
| `GET` | `/auth/oidc/start` | `303` to the provider |
| `GET` | `/auth/oidc/callback` | `303` to `/` after session creation |
| `GET` | `/api/v1/session` | `200` session user and CSRF token; otherwise `401` |
| `DELETE` | `/api/v1/session` | revoke and clear cookie, `204` |

Logout is idempotent from the user's perspective but requires valid CSRF and
Origin when a session cookie is present.

Successful session representation:

```json
{
  "user": {
    "id": "42",
    "display_name": "Yuito",
    "created_at_ms": 1781971200000
  },
  "csrf_token": "opaque-value"
}
```

Use explicit transport DTOs in `chat-server`; do not derive Serde traits on
core domain types. Convert between the two at the handler boundary.

### JSON rules

- UTF-8 JSON with `application/json` for normal representations.
- snake_case field and enum names.
- Every database ID is a positive base-10 JSON string.
- Millisecond Unix timestamps are JSON integers; current values are safely
  below JavaScript's exact-integer limit.
- `null` is used only where absence is part of the contract.
- Collections are arrays, including empty collections.
- Request DTOs reject unknown fields; response consumers must tolerate added
  fields within the same protocol version.
- Apply a small explicit body limit, initially 16 KiB, to JSON command routes.
- Return `413` for oversized input and `415` for a wrong content type.
- Add `Cache-Control: no-store` to session and authenticated API responses.

### Errors

Use RFC 9457 problem details with `application/problem+json`. Problem identity
is the stable absolute URN in `type`, for example:

```json
{
  "type": "urn:chat-rs:problem:authentication-required",
  "title": "Authentication required",
  "status": 401
}
```

Define and test a finite mapping rather than serializing Rust error text:

| Condition | Status | Problem suffix |
| --- | --- | --- |
| no valid session | `401` | `authentication-required` |
| authenticated but forbidden | `403` | `forbidden` |
| resource hidden or absent | `404` | `not-found` |
| malformed JSON or ID | `400` | `invalid-request` |
| domain validation failure | `422` | `validation-failed` |
| state conflict | `409` | `conflict` |
| wrong content type | `415` | `unsupported-media-type` |
| oversized request | `413` | `payload-too-large` |
| temporary store/provider failure | `503` | `service-unavailable` |

Do not include SQL errors, paths, provider responses, token claims, stack
traces, or internal invariant failures. Unexpected invariant failures are
logged and returned as a generic `500` problem.

## 4B HTTP Chat Surface

After 4A passes, map the existing core capabilities through authenticated HTTP
routes. The initial candidate surface is:

```text
GET    /api/v1/conversations
POST   /api/v1/conversations
GET    /api/v1/conversations/{conversation_id}
GET    /api/v1/conversations/{conversation_id}/members
DELETE /api/v1/conversations/{conversation_id}/members/{user_id}
GET    /api/v1/conversations/{conversation_id}/messages?before=...&limit=...
POST   /api/v1/conversations/{conversation_id}/messages
```

The authenticated user is always the actor. Resource IDs are parsed from
decimal path/query strings into the existing strong ID types. Message pages
preserve newest-first ordering and string cursors.

Do not expose add-member yet. The core accepts a `UserId`, but the product has
no safe user-discovery or invitation flow, and display names are not unique.
Choose an invitation model or a deliberately scoped user directory before
making that operation reachable from the browser.

POST retry semantics also require an explicit decision before 4B. The current
IETF `Idempotency-Key` proposal is an expired Internet-Draft, not a published
standard. Prefer entity-specific client operation IDs stored in the same SQLite
transaction for message and conversation creation rather than promising a
generic idempotency layer that cannot be atomic with existing use cases.

## Future WebSocket Contract

Milestone 5 will use `/api/v1/ws` and negotiate the `chat.v1` WebSocket
subprotocol. The upgrade authenticates the existing session cookie and validates
Origin before switching protocols.

WebSocket is initially a server-to-client live event channel. Commands and
queries remain HTTP. This avoids inventing separate request correlation,
idempotency, validation, and error semantics for WebSocket messages.

Live events are not the durable source of truth. The first implementation has
no persisted global event sequence, so it must not promise lossless replay or a
global resume cursor. After reconnect, the client refreshes snapshots and
message history over HTTP; SQLite entity/message IDs provide the durable state.

Session expiry and revocation for already-upgraded connections, bounded outbound
queues, heartbeat timeout, slow-consumer policy, and graceful connection drain
are Milestone 5 concerns.

## Rust Dependencies

Research as of 2026-06-21 supports these direct dependencies for 4A:

```toml
axum = { version = "0.8.9", default-features = false, features = [
    "http1",
    "json",
    "query",
    "tokio",
    "tracing",
] }
base64 = "0.22.1"
cookie = "0.18.1"
getrandom = "0.4.3"
openidconnect = { version = "4.0.1", default-features = false, features = [
    "reqwest",
    "rustls-tls",
] }
serde = { version = "1.0.228", features = ["derive"] }
serde_json = "1.0.150"
sha2 = "0.10.9"
subtle = "2.6.1"
url = "2.5.8"
```

Use SHA-2 0.10 rather than 0.11 because `openidconnect` 4 uses the compatible
0.10 series; this avoids compiling two major digest stacks for one binary.

Reasons:

- `openidconnect` implements the standard flow, discovery, typed secrets, PKCE,
  nonce, signature, issuer, and audience validation. Do not hand-roll JWT/OIDC.
- `getrandom` directly fills session and CSRF token bytes from the OS CSPRNG.
- `sha2` hashes bearer tokens before persistence and secret comparison.
- `subtle` performs best-effort constant-time comparison of fixed-size secret
  hashes such as the CSRF proof.
- `base64` provides explicit URL-safe, no-padding encoding.
- `cookie` parses and formats cookie attributes without adding framework-owned
  session behavior.
- Serde and Axum JSON provide strongly typed DTO parsing rather than dynamic
  `serde_json::Value` access.
- `url` validates the public origin and redirect URI composition.

Do not add `jsonwebtoken`, `tower-cookies`, `axum-extra`, `async-trait`, a generic
session framework, or a second crate in 4A. They either duplicate OIDC behavior,
hide security policy, or add an abstraction with no current second consumer.

## Module Layout

Keep implementation inside `chat-server`:

```text
crates/chat-server/src/
|-- app.rs
|-- auth.rs                 # verified identity and session service
|-- auth/
|   |-- cookie.rs           # cookie construction and parsing policy
|   |-- oidc.rs             # provider adapter and redirect flow
|   |-- session.rs          # opaque token and CSRF handling
|   `-- store.rs            # auth persistence operations
|-- config.rs
|-- lib.rs
|-- main.rs
|-- server.rs
|-- sqlite.rs
`-- sqlite/
```

Exact file splitting follows code size. Module ownership matters more than
creating every listed file immediately. `server.rs` is the runtime composition
root; `main.rs` is limited to process configuration, telemetry, and exit status.

## Test Plan

### Configuration

- valid HTTPS and loopback HTTP public URLs
- rejection of non-loopback HTTP, path, query, fragment, and credentials
- OIDC disabled, fully configured, and partially configured states
- client secret never appears in Debug or errors

### Identity and persistence

- same authority and subject resolve to the same user
- same subject under different authorities resolves independently
- email/profile changes do not change identity or display name
- concurrent first login creates one user and one binding
- user and identity creation rollback together on failure
- invalid stored values are rejected

### Sessions

- tokens contain 256 random bits before encoding
- raw session bearer values are never persisted
- valid, expired, malformed, missing, and revoked sessions
- session rotation on login and idempotent logout
- independent browser sessions for one user
- CSRF validation and rotation
- exact cookie attributes for HTTPS and loopback development

### OIDC

Use an in-process local provider fixture; never call a public provider in tests.

- discovery and redirect URL construction
- state, browser binding, nonce, and PKCE success
- missing, mismatched, expired, and replayed transaction rejection
- issuer, audience, expiry, signature, and nonce failures
- metadata redirects are refused
- bounded metadata refresh on signing-key rotation
- provider errors do not disclose response bodies or secrets

### HTTP protocol

Use Tower `oneshot` for handler contracts.

- exact success JSON and string ID representation
- problem status, content type, and stable type URN
- malformed JSON, unknown fields, media type, and body limit
- authenticated actor comes only from request extensions/session
- `Cache-Control: no-store`
- unsafe methods require CSRF and exact Origin
- health routes remain unauthenticated

Use exact `serde_json::Value` assertions or checked fixture files. Do not add a
snapshot-test dependency for this small protocol.

## Implementation Slices

### Slice 1: Configuration and protocol primitives

- add researched dependencies and minimal features
- add public URL and OIDC configuration
- add explicit session/user DTOs and RFC 9457 problem responses
- enable and limit Axum JSON extraction

Completion: configuration and serialization contracts are pure and fully
tested, with no authentication network I/O yet.

### Slice 2: Authentication schema and session service

- add migration 0002
- add token generation, hashing, session create/resolve/revoke
- add cookie and CSRF policy
- add persistence and concurrency tests

Completion: a test-created `UserId` can receive a secure session and be resolved
through the same extractor future HTTP handlers will use.

### Slice 3: Identity resolution

- add `VerifiedIdentity`
- atomically resolve or provision users and identity bindings
- reuse core display-name validation and factored SQLite user insertion
- test races and provider-claim changes

Completion: verified identity is cleanly separated from account and session,
and concurrent provisioning is deterministic.

### Slice 4: OIDC adapter

- initialize discovery and the no-redirect async HTTP client
- add start and callback transaction flow
- validate code, state, binding, PKCE, ID token, and nonce
- add local provider fixtures and key-refresh tests

Completion: a standards-conforming provider can establish `VerifiedIdentity`
without exposing provider types to other modules.

### Slice 5: Session HTTP resource and request protection

- add authenticated extractor
- add GET and DELETE session routes
- apply CSRF, Origin, no-store, and problem mappings
- compose auth state into the router without changing health behavior

Completion: a browser session has an explicit tested lifecycle and application
handlers derive their actor only from the authenticated request context.

### Slice 6: Verification and documentation

- run formatting, Clippy, workspace tests, docs, and locked release build
- manually verify disabled and configured startup modes
- verify cookie attributes and logout in a real browser
- verify OIDC against one configured provider
- update README status and operational configuration

Completion: 4A is usable with a real OIDC provider and is independently
reviewable before chat HTTP routes are exposed.

## Acceptance Criteria for 4A

- `chat` contains no authentication or transport dependency
- an OIDC adapter maps only validated issuer/subject identity into the common
  authority/subject key
- identity binding and first-user provisioning are atomic
- sessions are random, opaque, hashed at rest, expiring, and revocable
- cookies have the documented host, Secure, HttpOnly, SameSite, and path policy
- unsafe cookie-authenticated requests require CSRF and exact Origin
- OIDC flow validates state, binding, PKCE, nonce, issuer, audience, signature,
  and expiry
- no provider access or refresh token is persisted
- `/api/v1/session` follows the documented JSON and problem contracts
- secrets and internal errors never enter HTTP bodies or logs
- health behavior and graceful shutdown remain unchanged
- all workspace checks and real-provider manual verification pass

## Primary References

- [OpenID Connect Core 1.0](https://openid.net/specs/openid-connect-core-1_0-18.html)
- [OAuth 2.0 Security Best Current Practice, RFC 9700](https://www.rfc-editor.org/rfc/rfc9700.html)
- [Proof Key for Code Exchange, RFC 7636](https://www.rfc-editor.org/rfc/rfc7636.html)
- [`openidconnect` 4.0.1](https://docs.rs/openidconnect/4.0.1/openidconnect/)
- [The WebSocket Protocol, RFC 6455](https://www.rfc-editor.org/rfc/rfc6455.html)
- [HTTP State Management, RFC 6265](https://www.rfc-editor.org/rfc/rfc6265.html)
- [Cookie specification draft](https://datatracker.ietf.org/doc/html/draft-ietf-httpbis-rfc6265bis-22)
- [OWASP Session Management](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html)
- [OWASP CSRF Prevention](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html)
- [JSON, RFC 8259](https://www.rfc-editor.org/rfc/rfc8259.html)
- [Problem Details for HTTP APIs, RFC 9457](https://www.rfc-editor.org/rfc/rfc9457.html)
- [Axum JSON](https://docs.rs/axum/0.8.9/axum/struct.Json.html)
- [Axum body limits](https://docs.rs/axum/0.8.9/axum/extract/struct.DefaultBodyLimit.html)
- [Serde 1.0.228](https://docs.rs/serde/1.0.228/serde/)
- [`getrandom` 0.4.3](https://docs.rs/getrandom/0.4.3/getrandom/)
- [`cookie` 0.18.1](https://docs.rs/cookie/0.18.1/cookie/)
- [`subtle` 2.6.1](https://docs.rs/subtle/2.6.1/subtle/)
