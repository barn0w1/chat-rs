# E2E Verification Report

Status: passed
Date: 2026-06-22
Target: release build of `chat-server`
Database: dedicated disposable SQLite E2E database
Public origin: `https://chat.hss-science.org`

This is the repository-safe evidence summary. External account identifiers,
bearer values, identity subjects, cookies, CSRF tokens, OIDC parameters, and
admission codes are intentionally omitted.

## Preconditions

- Formatting, check, Clippy with warnings denied, and all workspace tests
  passed locally.
- Caddy terminated public TLS and proxied to the loopback listener.
- The operator reviewed the effective Caddy access-log and external shipping
  policy before real login.
- Application request spans contained method, matched route template, status,
  and latency, with no query or request headers.
- Separate browser profiles represented accounts A, B, and C.

The original developer-console form utility was rejected with `Origin: null`.
The final run used a temporary static HTML form served by Caddy from the exact
public origin. Its POST carried the expected origin and the
`Sec-Fetch-Site: same-origin` header. The temporary route was removed after
verification.

## Results

| Case | Contract | Result |
| --- | --- | --- |
| E0 | Public live and ready probes return `204` through Caddy | Passed |
| E1 | Verified unknown identity without admission is denied with `401` and creates no user | Passed |
| E2 | Invalid admission code and invalid Origin are rejected with `403` | Passed |
| E3 | A reusable expiring code can be created while the server remains live | Passed |
| E4 | Code admits A, callback creates a secure host-only session | Passed |
| E5 | The same active code independently admits B | Passed |
| E6 | Existing A resolves to the same user without a code, including after restart | Passed |
| E7 | Session read succeeds; mutation without CSRF proof is rejected | Passed |
| E8 | Authenticated conversation creation, message posting, and history read succeed | Passed |
| E9 | Logout revokes the session and removes the browser cookie | Passed |
| E10 | `open` admits C; all existing identities remain usable after restoring `invite_only` | Passed |

## Boundary Evidence

- Google discovery accepted the exact issuer
  `https://accounts.google.com`.
- Unknown authentication under `invite_only` did not create `users` or
  `auth_identities` rows.
- One active admission code admitted two independent identities without a
  server restart and remained reusable until expiry.
- Secure cookies were `HttpOnly`, `Secure`, `SameSite=Lax`, host-only, and
  scoped to `/`.
- Identity-to-user bindings and user IDs survived graceful shutdown and
  restart.
- API IDs were serialized as JSON strings.
- Mutation requests required both exact Origin and CSRF proof.
- Request logs used route templates such as
  `/api/v1/conversations/{conversation_id}/messages` and excluded queries.
- No unexpected `5xx`, panic, or secret-bearing application log entry was
  observed.

## Final Persistent State

| Resource | Count | Expected state |
| --- | ---: | --- |
| Migrations | 4 | Versions 1 through 4 applied successfully |
| Users | 3 | A, B, and C mapped to distinct integer IDs |
| Identity bindings | 3 | One validated Google subject per user |
| Sessions | 5 | Multiple unexpired login sessions from the test sequence |
| Conversations | 1 | E2E conversation owned by A |
| Memberships | 1 | A is the conversation owner |
| Messages | 1 | One E2E message by A |
| Admission codes | 2 | Reusable test codes pending expiry or disposal |
| Pending OIDC logins | 0 | Completed transactions consumed |

## Conclusion

The production-like E2E gate passed. The deployed system demonstrated the
implemented origin, identity, admission, session, CSRF, persistence, HTTP API,
logging, and graceful-restart contracts through the intended public topology.

The server was restored to `invite_only`, the temporary Caddy form route was
removed, the admission codes were discarded, unnecessary provider test access
was removed, and the disposable database was scheduled for deletion after its
evidence retention period.
