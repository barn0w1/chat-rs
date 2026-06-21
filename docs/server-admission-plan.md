# Server Admission Plan

Status: implemented; local Rust and real-provider verification pending  
Date: 2026-06-22

## Purpose

Server admission answers one question:

> May this verified external identity become a user of this self-hosted server?

It does not authenticate the subject and does not authorize conversation
operations.

```text
credential evidence
        |
        v
authentication adapter
        |
        v
VerifiedIdentity             authentication
        |
        v
existing identity binding?   account resolution
        |
        v
admission policy             new-user admission only
        |
        v
UserId + session
        |
        v
conversation membership      application authorization
```

Anyone may complete authentication without an admission code. This is safe:
`VerifiedIdentity` only states that the server has verified continuity of an
external subject through a trusted adapter. It grants no chat access by itself.

The server operator chooses both the trusted authentication adapter and the
admission policy. OIDC is the first adapter, not part of the admission model.
The existing opaque `(authority, subject)` value remains the boundary for any
future adapter.

## Scope

Implement:

- `open` and `invite_only` modes;
- reusable, expiring admission codes;
- one operator command to create a code with a chosen lifetime;
- safe carriage of a code reference through the existing OIDC transaction;
- atomic new-user admission; and
- policy, expiry, rollback, and concurrency tests.

Do not implement:

- an explicit `closed` mode;
- per-person or single-use invitations;
- usage counts, reservations, or pending-admission sessions;
- usernames or profile editing;
- administrator roles or an administration UI;
- account linking, recovery, merging, or provider migration;
- email/domain allowlists or approval queues;
- user suspension or session administration;
- conversation membership workflows; or
- WebSocket delivery.

Do not add an `Authenticator` trait before a second adapter demonstrates a
shared behavioral interface. A future adapter only needs to produce
`VerifiedIdentity` and carry an optional method-independent admission-code
reference through its own authentication flow.

## Policy

Add `CHAT_ADMISSION_MODE`:

| Mode | Existing identity | New verified identity |
| --- | --- | --- |
| `open` | allow | create automatically |
| `invite_only` | allow | require an unexpired admission code |

The default is `invite_only`. When no unexpired code exists, no new identity
can join and the server is effectively closed. Existing users remain able to
authenticate.

An explicit `closed` value would not add behavior in this milestone. The
operator closes admission by not creating another code or by allowing the
current code to expire.

Mode and code expiry are checked only when creating a new local user. Changing
the mode or reaching expiry does not revoke existing users or sessions.

## Admission Code

An admission code is a shared, time-bounded bearer authorization grant. It is
not tied to a person or `VerifiedIdentity` and is not an authentication factor.
The operator may send the same code to a group. Every independently verified
identity that presents it before expiry may join.

The only configurable restriction is expiry. A code has no use count and is
not consumed on successful admission.

Each code is:

- generated from 32 random bytes through the existing `SecretToken` mechanism;
- encoded as unpadded URL-safe Base64 at the terminal/browser boundary;
- stored only as `SHA-256(token)`;
- assigned an operator-selected positive lifetime in whole hours; and
- rejected at or after its absolute expiry time.

The raw code must never appear in SQLite or server logs. OWASP guidance for
comparable bearer tokens recommends secure random generation, sufficient
length, secure storage, and expiry. Single use is useful for password recovery,
but it is intentionally not part of this shared admission-code contract.

The accepted operational risk is explicit: anyone who obtains the code and can
produce a `VerifiedIdentity` may join until it expires. This is the intended
tradeoff for low operator effort. Choose a shorter lifetime when distributing a
code broadly. Revocation can be added later if actual operation shows that
expiry alone is insufficient.

## Operator Workflow

Provide one command:

```text
chat-server admission-code create --valid-for-hours <positive integer>
```

Example:

```text
chat-server admission-code create --valid-for-hours 168
```

The command opens and migrates the configured SQLite database, creates the
code, and prints the code and absolute expiry exactly once. It can run while
the server is using SQLite.

The grammar is small and fixed, so parse it with `std::env::args_os`. Reject
missing, zero, non-integer, duplicate, or overflowing lifetimes. Cap the
lifetime at a documented constant, initially 8,760 hours (one year). A CLI
framework, listing command, revocation command, and administration API are not
needed for this increment.

Creating one shared code is sufficient for a group. The operator does not need
to generate or track a separate secret for each person.

## Authentication and Admission Flow

### Authentication without a code

The current `GET /auth/oidc/start` route remains available in both modes.

1. The adapter validates the authentication response.
2. It produces `VerifiedIdentity` without consulting admission policy.
3. If `(authority, subject)` is already bound, issue a normal session.
4. If it is new, admit it only in `open`; otherwise return the finite login
   failure without creating a `User`, binding, or session.

Thus any subject can be verified without an admission code, while only an
admitted subject can use the application.

### Authentication with a code

1. The user enters the shared code on `/join`.
2. A top-level form sends `POST /auth/oidc/start` with
   `application/x-www-form-urlencoded`.
3. The server requires an exact `Origin` match, hashes the code, and resolves
   one unexpired code row.
4. The server stores only its internal `AdmissionCodeId` in the existing OIDC
   login transaction, then redirects to the provider.
5. The adapter completes authentication and produces `VerifiedIdentity`.
6. The admission operation receives that identity and optional internal code
   ID. It first resolves an existing binding.
7. For a new identity in `invite_only`, it rechecks that the referenced code is
   unexpired inside the admission transaction.
8. It atomically creates the `User` and identity binding. It does not modify or
   consume the code.
9. The HTTP boundary issues a session only after commit.

Entering the code before the provider redirect is a browser-flow detail, not a
policy decision. Code validation that authorizes user creation occurs only
after `VerifiedIdentity` exists.

The raw code is never placed in a URL, cookie, OIDC parameter, or login
transaction. OWASP warns that URL secrets can leak through history, logs, and
referrers, and recommends POST plus server-side CSRF defenses for state-changing
operations. The form POST therefore requires exact Origin validation. Existing
OIDC state, browser-binding cookie, nonce, and PKCE checks remain unchanged.

If a code expires during provider authentication, the new identity is denied.
The user can retry with a current code. Concurrent new identities may all use
the same unexpired code; this is expected behavior.

## Persistence

Add `0003_server_admission.sql`:

```text
admission_codes
|-- id INTEGER PRIMARY KEY
|-- token_hash BLOB UNIQUE, exactly 32 bytes
|-- created_at_ms INTEGER, non-negative
`-- expires_at_ms INTEGER, greater than created_at_ms

oidc_login_transactions
`-- admission_code_id INTEGER nullable -> admission_codes(id) ON DELETE SET NULL
```

Add an expiry index. Delete expired codes opportunistically when creating a new
code. Lookup remains read-only and filters by expiry, so unauthenticated invalid
submissions cannot force a SQLite write lock. Existing identity bindings are
already admitted; no backfill or additional account-status table is required.

The callback must recheck expiry in the final admission transaction. Validation
at login start improves feedback but is not authorization. `BEGIN IMMEDIATE`
and the existing unique identity key remain the concurrency boundary.

If session insertion fails after admission commits, the user is admitted but
not logged in. A later authentication resolves the binding and can issue the
session. This is a safe, recoverable state.

## Rust Boundaries

Server admission belongs to `chat-server`, not the `chat` crate.

```text
config.rs
`-- AdmissionMode

auth/admission.rs
|-- AdmissionCodeId
`-- AdmissionOutcome

auth/store.rs
|-- create_admission_code
|-- resolve_admission_code
`-- resolve_or_admit

http/session.rs
|-- GET  /auth/oidc/start
|-- POST /auth/oidc/start
`-- callback translation

command.rs
`-- admission-code create
```

The central operation is method-independent:

```text
resolve_or_admit(
    identity: VerifiedIdentity,
    code: Option<AdmissionCodeId>,
    mode: AdmissionMode,
) -> Admitted(User) | Denied
```

Rename the current `resolve_or_provision` so that its policy decision is
explicit. The OIDC layer carries an opaque internal code ID but does not decide
admission. Admission code types contain no OIDC claim, authorization code, or
token.

Add Axum's existing `form` feature for the join form. No new runtime library is
otherwise required.

## Required Invariants

- Producing `VerifiedIdentity` never requires an admission code.
- Existing identity bindings always bypass new-user admission checks.
- `invite_only` creates no new user without an unexpired code.
- A valid code may admit any number of verified identities before expiry.
- Code expiry never removes an existing user or session.
- User creation and identity binding commit or roll back together.
- Concurrent first logins for one identity converge on one local user.
- Raw authentication and admission secrets never enter logs or persistent
  plaintext.
- Public failures do not expose whether an identity is already registered.

## Test Plan

Configuration and value tests:

- parse `open` and `invite_only`, defaulting to `invite_only`;
- reject invalid modes and invalid lifetime arguments;
- enforce the maximum code lifetime; and
- reject wrong Origin, content type, malformed form, and oversized input.

SQLite tests:

- store only the code hash and resolve it before expiry;
- reject it at and after expiry;
- reuse one code for multiple different verified identities;
- preserve existing-user login after code expiry;
- auto-create without a code only in `open`;
- create no user on missing or invalid code in `invite_only`;
- roll back an injected write failure; and
- converge concurrent first logins for one identity.

Router tests:

- preserve ordinary GET login without a code;
- verify an unknown identity before denying it in `invite_only`;
- store only `AdmissionCodeId` in the OIDC transaction;
- reject an invalid code POST before provider redirect;
- set no application-session cookie on admission denial; and
- issue sessions for existing, open, and valid-code cases.

In-process Axum and SQLite tests cover the mechanical contract. A real provider
is still needed to manually verify existing login, open registration,
shared-code admission, missing code, and expiry through a browser.

## Implementation Sequence

1. Add `AdmissionMode`, defaulting to `invite_only`.
2. Add the code table and optional OIDC transaction reference migration.
3. Implement code creation, hash lookup, expiry, and cleanup.
4. Add the operator command and its bounded lifetime parser.
5. Replace `resolve_or_provision` with `resolve_or_admit`.
6. Implement and test existing, `open`, and `invite_only` outcomes.
7. Carry optional `AdmissionCodeId` through the OIDC transaction.
8. Add the form POST route, body limit, and exact Origin validation.
9. Add rollback, concurrency, router, and regression tests.
10. Update operator documentation and run manual acceptance checks.

## Acceptance Criteria

- Authentication and admission are separate in types and control flow.
- The default server admits no new user without an unexpired code.
- One shared code can admit a group until its configured expiry.
- The operator creates and distributes one code, not one code per person.
- No code means effectively closed registration.
- Existing users remain unaffected by mode and code expiry.
- The admission layer contains no OIDC-specific type or behavior.
- There is no pending-admission subsystem or usage-count state.
- All admission writes are atomic.
- Formatting, check, Clippy with warnings denied, and all tests pass.

## Sources

- [OpenID Connect Core 1.0](https://openid.net/specs/openid-connect-core-1_0.html)
- [NIST SP 800-63C-4, Federation and Assertions](https://pages.nist.gov/800-63-4/sp800-63c.html)
- [OWASP bearer-token properties](https://cheatsheetseries.owasp.org/cheatsheets/Forgot_Password_Cheat_Sheet.html)
- [OWASP CSRF Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html)
- [SQLite transaction behavior](https://www.sqlite.org/lang_transaction.html)
