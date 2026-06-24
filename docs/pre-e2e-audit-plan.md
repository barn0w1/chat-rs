# Pre-E2E Audit and Hardening Plan

Status: implemented, locally verified, and followed by passing production-like
E2E verification on 2026-06-22
Date: 2026-06-22
Authentication decision revised: 2026-06-22

## Purpose

Before exercising the public deployment with a real Google account, harden the
complete path from an unauthenticated HTTP request to persisted chat state. The
objective is a focused review increment, not a broad refactor.

The server-side OpenID Connect Authorization Code Flow remains the selected
authentication method. The browser follows redirects but does not receive,
validate, retain, or submit an ID token. Provider code exchange, ID-token
verification, identity derivation, admission, and application-session issuance
remain server responsibilities.

E2E starts only after this plan's acceptance criteria pass locally.

The implementation adds path-only matched-route tracing, non-cacheable
authentication redirects, strict security-cookie extraction, bounded callback
values, same-browser login replacement, a 1,024-live-login ceiling, and a
unique browser-binding index in migration `0004_oidc_login_capacity.sql`.

## Reviewed Surface

The review covered:

- every module and test in `crates/chat`;
- server composition, configuration, lifecycle, tracing, and operator commands;
- HTTP routing, extraction, error translation, representations, cookies,
  Origin, CSRF, and request-body limits;
- OIDC discovery, state, nonce, PKCE, code exchange, identity derivation, and
  session issuance;
- admission-code generation, carriage, expiry, and atomic admission;
- all three SQLite migrations and the read/write adapters; and
- the current operational and E2E documentation.

This is a source and design audit, not a penetration test or a substitute for a
RustSec advisory scan. The local Rust checks previously reported by the project
owner remain the mechanical baseline; this environment does not contain Cargo.

## Architecture Decision

The authentication boundary remains:

```text
browser navigation
        |
        v
server creates state + browser binding + nonce + PKCE
        |
        v
provider authorization endpoint
        |
        v
server callback receives one-time authorization code
        |
        v
server exchanges code and validates ID token
        |
        v
VerifiedIdentity (authority, subject, display name)
        |
        v
existing binding / admission policy
        |
        v
opaque server session
        |
        v
authenticated HTTP use case
        |
        v
chat core + transactional store
```

This is a deliberate server/BFF-style ownership decision:

- authentication protocol state and provider credentials stay in one process;
- ID tokens and provider access tokens do not enter application JavaScript;
- the browser receives only the application's `HttpOnly` opaque session cookie;
- the Web Client does not need a Google SDK or JWT handling code;
- admission remains independent of the authentication method; and
- a future authentication adapter can still produce the same
  `VerifiedIdentity` without changing `chat`.

The authorization-code flow is more code than direct GIS credential POST, but
here that complexity is contained in `chat-server` instead of being divided
between JavaScript and Rust. Google documents the server flow specifically for
a backend server verifying the identity of a browser user. It requires a
high-entropy anti-forgery `state`, exact redirect URI, one-time code exchange,
and ID-token validation. OpenID Connect defines `nonce` to bind the client
session to the ID token, and current OAuth security guidance recommends PKCE for
authorization-code flows. The existing implementation already applies all
three protections.

Do not add the GIS credential flow alongside this one. Two first-party login
paths would increase attack surface and verification work without a current
requirement.

## Current Assessment

The `chat` crate is not a current hardening concern. It validates commands
before calling narrow store capabilities, checks returned store state,
separates reads from mutations, and leaves authentication, transport, and
transactions outside the core. SQLite performs authorization-sensitive
mutations in immediate transactions and reconstructs validated domain values
at the persistence boundary.

The authentication and admission implementation has sound properties:

- external identity is keyed by `(authority, subject)`, not mutable email;
- session, login-state, browser-binding, and admission tokens use 32 random
  bytes;
- bearer session, state, binding, and admission values are hashed at rest;
- secret value types redact `Debug` output;
- login state is single-use and bound to a browser cookie;
- ID-token issuer, audience, signature, expiry, and nonce are verified by the
  maintained `openidconnect` library;
- the authorization code is bound with PKCE before exchange;
- provider HTTP requests have connection and total timeouts and do not follow
  redirects;
- new-user admission and identity binding are atomic;
- session rotation removes the previous presented session in the insertion
  transaction; and
- public failures do not disclose whether an external identity already exists.

No defect was found that justifies restructuring the core or SQLite chat
adapter before E2E.

## Findings and Decisions

### P0: Request targets can expose authentication secrets

`DefaultMakeSpan` records the full request URI. The OIDC callback places `code`
and `state` in its query, so ordinary application logs can retain one-time
credentials and correlation material. Caddy access logs can independently do
the same.

Replace the default span builder with a small application-owned builder that
records only:

- HTTP method; and
- Axum `MatchedPath` (for example,
  `/api/v1/conversations/{conversation_id}`), falling back to `uri.path()` only
  when no route matched.

Never record query, headers, request or response bodies, cookies, redirect
locations, authorization codes, state, nonce, PKCE values, tokens, or identity
claims in the generic request span. A route template is preferred over the
concrete path because it also avoids user-data leakage and unbounded metric
cardinality.

The application cannot enforce Caddy logging. The server operator owns that
policy. Before E2E, inspect the effective Caddy configuration and make an
explicit decision: omit or redact callback query values (recommended), disable
callback access logging, or accept tightly controlled retention. The E2E log
review must check both layers against the selected policy.

### P1: Authentication responses need an explicit cache policy

JSON application and problem responses already use `Cache-Control: no-store`,
but authentication redirects do not. Redirects can be stored under HTTP cache
rules, and their `Location` contains short-lived OIDC request values.

Every login-start redirect, callback outcome, login failure, session response,
logout response, and cookie-mutating response must carry
`Cache-Control: no-store`. Centralize this in response helpers so a newly added
auth outcome cannot omit it accidentally.

### P1: Duplicate authentication cookies are accepted ambiguously

Current cookie lookup returns the first matching name across all Cookie header
fields and silently skips malformed cookie pairs. Different intermediaries can
select a different duplicate, which creates parser ambiguity at a security
boundary.

Authentication cookie extraction must accept exactly one well-formed cookie
with the requested name. Missing is unauthenticated; malformed or duplicated is
rejected. Continue using `__Host-` names, `Secure`, `HttpOnly`, `SameSite=Lax`,
and `Path=/` for HTTPS.

### P1: Login initiation consumes public write capacity

`GET /auth/oidc/start` creates a short-lived SQLite transaction row. Automated
requests can therefore grow the set of live login rows during the ten-minute
validity window and contend for SQLite's single writer.

Keep browser navigation simple and retain the GET start endpoint, but bound its
state:

- opportunistically delete expired login rows before insertion;
- reuse a valid existing browser-binding cookie when starting another login;
- replace that browser binding's previous live transaction atomically;
- add a unique index for live ownership by browser-binding hash if required by
  the final transaction design; and
- enforce a documented global ceiling for unexpired login transactions,
  failing without another insertion when the ceiling is reached.

The global ceiling protects finite SQLite capacity; it is not an authentication
or admission decision. Do not base correctness on a forwarded client IP. Caddy
may additionally apply coarse abuse controls as an operational layer.

### P1: The callback boundary needs explicit size and shape constraints

The current callback correctly ignores unrecognized response parameters, as
required for OAuth authorization responses. It also parses `state` into the
fixed-size `SecretToken` before lookup and consumes the transaction once.

Add explicit upper bounds for callback `code`, `state`, and provider `error`
values before token exchange or diagnostic handling. Reject duplicated known
parameters and malformed query encoding with the same finite login-failure
response. Do not log the rejected values. URI/header limits at Caddy remain an
outer resource boundary, while semantic limits belong in the application.

Keep the current security order:

1. parse a finite callback;
2. require state and browser-binding cookie;
3. atomically validate and consume the login transaction;
4. handle provider denial or require one code;
5. exchange with the stored PKCE verifier;
6. validate the ID token with the stored nonce;
7. resolve/admit the verified identity; and
8. rotate the application session.

### P1: Provider failures must remain credential-free

The current `OidcError` deliberately discards token-endpoint response details
for public and application logs. Preserve that property. Metadata refresh may
log only a fixed event and must stay rate-bounded. Never enable HTTP client body
logging or attach provider request/response objects to tracing spans.

The server does not need Google access or refresh tokens. Continue requesting
only identity scopes, do not request offline access, and discard the token
response after validated claims are derived.

### P2: Error classification can be more precise

Authentication errors currently become `503` uniformly. Keep public details
finite, but distinguish temporary provider/store failure (`503`) from invalid
persisted invariants or impossible internal state (`500`) in one mapping
function. Logs may contain the safe error category and source error, never
credentials.

### P2: Operational controls need an explicit ownership boundary

Connection/header timeouts, TLS, HSTS, access-log policy, and coarse abuse
controls are deployment concerns because Caddy is the public HTTP endpoint.
JSON/form body limits, Origin/CSRF checks, OIDC verification, callback semantic
limits, and database capacity invariants remain application concerns.

Record the required Caddy behavior for E2E, but do not add proxy-header trust,
generic rate-limit middleware, CSP, compression, or request IDs in this
increment. CSP belongs with the actual embedded Web Client; request IDs should
be added only with an end-to-end Caddy propagation policy.

## Implementation Plan

### Increment 1: Safe observability and responses

1. Replace `DefaultMakeSpan` with method plus matched-route tracing.
2. Add focused tests proving a URI containing `?code=...&state=...` contributes
   only the route/path field and that headers remain absent.
3. Add `no-store` to every authentication/session response helper.
4. Update the Caddy prerequisite and log-inspection procedure.

This increment is independently mergeable and does not alter authentication
semantics.

### Increment 2: Harden the existing server-side OIDC flow

1. Add strict callback DTO parsing and bounded values without rejecting unknown
   extension parameters.
2. Make login start replace an earlier transaction for the same valid browser
   binding.
3. Add bounded live-login capacity and deterministic expiry cleanup.
4. Preserve discovery, no-redirect provider HTTP, timeouts, state, browser
   binding, nonce, PKCE, code exchange, ID-token verification, and bounded key
   refresh.
5. Confirm Google deployments require the configured confidential-client
   secret while retaining the existing generic public-client capability only if
   `openidconnect` and provider metadata support it correctly.
6. Confirm that no access or refresh token is stored or exposed.
7. Preserve the current admission-code reference through the login transaction
   and recheck expiry in the atomic admission operation.

No GIS JavaScript, credential POST endpoint, raw-ID-token request body, or new
client-side authentication dependency is introduced.

### Increment 3: Boundary cleanup

1. Make security-cookie extraction strict and test duplicate/malformed input.
2. Split internal and unavailable authentication error mappings.
3. Audit every route for media type, body size, query/path parsing, cache policy,
   authentication, and mutation CSRF requirements.
4. Remove only code made obsolete by these hardening changes. Do not rewrite the
   stable core, persistence adapter, admission model, or session model.

### Increment 4: Verification gate

Run:

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo audit --locked
```

`cargo audit` is an additional advisory check and does not replace locked
compilation or tests. If it is not part of the developer toolchain, install and
run it explicitly for this review; do not silently add an unpinned CI installer.

After local verification, run the existing server-flow E2E cases through
`chat.hss-science.org` and Caddy. Update the E2E document only where hardening
changes its concrete request or evidence procedure.

## Required Tests

Observability and HTTP boundary:

- callback query values never enter application request spans;
- route templates, not concrete IDs, are logged for matched routes;
- login redirect, callback, failure, session, and logout responses are
  `no-store`;
- duplicate and malformed security cookies are rejected; and
- malformed, duplicate, or oversized callback values fail before provider or
  database work beyond the required state lookup.

OIDC protocol:

- state is random, hashed at rest, browser-bound, expires, and is consumed once;
- wrong binding, replayed state, expired state, provider denial, and missing code
  fail identically at the public boundary;
- PKCE verifier is applied to code exchange;
- invalid signature, issuer, audience, expiry, nonce, and missing ID token fail;
- metadata/key refresh is bounded and does not turn arbitrary failures into
  repeated discovery traffic;
- repeated login start for one browser does not accumulate live rows;
- the global live-login ceiling prevents another insertion; and
- code, state, nonce, PKCE, client secret, tokens, cookies, and claims do not
  appear in logs or public errors.

Admission and session regression:

- existing identities need no admission code;
- `open` admits a new verified identity;
- `invite_only` requires an unexpired shared code for a new identity;
- code expiry is rechecked in the final transaction;
- admission failure creates no user, binding, or session;
- successful login rotates a presented old session; and
- all existing chat authorization and persistence contract tests remain green.

Concurrency and recovery:

- concurrent callback consumption for one state produces at most one successful
  continuation;
- concurrent first login for one identity converges on one user;
- provider/store failure creates no application session; and
- session-insertion failure after committed admission remains recoverable by a
  later login.

## Deferred Work

Do not pull these into the pre-E2E change:

- GIS credential login or another authentication adapter;
- usernames, account linking, recovery, suspension, or provider migration;
- membership workflow expansion;
- WebSocket delivery, heartbeat, queues, and backpressure;
- durable event/outbox design for reconnect recovery;
- Web Client CSP and asset embedding;
- general HTTP rate limiting and request IDs; or
- backup, retention, and release packaging.

The later Milestone 5A realtime foundation uses in-memory `ChatEvent`
publication only as a notification source. It intentionally remains
non-durable; SQLite-backed HTTP state is still the recovery source.

## Acceptance Criteria

- Generic application request logs cannot contain a query or header value.
- The Caddy logging policy is explicitly reviewed. Callback query credentials
  are omitted/redacted, or their bounded residual risk, access control, and
  retention are deliberately accepted by the operator.
- No authentication or session response is cacheable.
- The only supported browser login is the server-side Authorization Code Flow.
- State, browser binding, nonce, PKCE, exact redirect URI, code exchange, and
  ID-token checks remain enforced.
- ID tokens and provider tokens never enter Web Client code or storage.
- Provider-specific evidence ends at `VerifiedIdentity`.
- Existing admission and opaque-session semantics remain unchanged.
- Authentication cookies have one unambiguous value.
- Public login initiation has bounded live SQLite state.
- All locked formatting, compilation, Clippy, tests, and advisory checks pass.
- The E2E procedure verifies the retained server flow with no secret-bearing
  log entry.

## Sources

- [Google OpenID Connect server flow](https://developers.google.com/identity/openid-connect/openid-connect)
- [OpenID Connect Core 1.0](https://openid.net/specs/openid-connect-core-1_0.html)
- [RFC 9700: OAuth 2.0 Security Best Current Practice](https://www.rfc-editor.org/rfc/rfc9700.html)
- [RFC 7636: Proof Key for Code Exchange](https://www.rfc-editor.org/rfc/rfc7636.html)
- [Axum `MatchedPath`](https://docs.rs/axum/0.8.9/axum/extract/struct.MatchedPath.html)
- [tower-http `DefaultMakeSpan`](https://docs.rs/tower-http/0.6.11/tower_http/trace/struct.DefaultMakeSpan.html)
- [RFC 9111: HTTP Caching](https://www.rfc-editor.org/rfc/rfc9111.html)
- [OWASP Logging Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Logging_Cheat_Sheet.html)
- [OWASP Session Management Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html)
- [RustSec `cargo-audit`](https://github.com/rustsec/rustsec/tree/main/cargo-audit)
