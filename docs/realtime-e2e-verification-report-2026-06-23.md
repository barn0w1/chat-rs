# Realtime E2E Verification Report

Status: passed
Date: 2026-06-23
Plan baseline: `58ba53e`
Public origin: `https://chat.hss-science.org`

This is the repository-safe evidence summary. Account identifiers, provider
subjects, cookies, CSRF tokens, admission codes, OIDC parameters, and raw
harness logs are intentionally omitted.

## Summary

The realtime E2E run verified the application-level WebSocket contract through
the public Caddy deployment:

- authenticated WebSocket upgrade works through the public origin;
- the `chat.v1` subprotocol is required and negotiated;
- unauthenticated and missing-protocol upgrades are rejected;
- `ready`, `subscribed`, `unsubscribed`, and expected rejection messages are
  observable;
- committed HTTP mutations produce expected realtime notifications;
- notifications can be reconciled through authenticated HTTP reads;
- reconnect does not imply durable replay, and HTTP recovery works;
- logout and server shutdown close sockets intentionally;
- malformed JSON and binary messages close the socket; and
- per-user connection limits are enforced.

The initial plan expected a second login as the same user from a different
browser profile to close the first profile's WebSocket. The run showed that
both sessions remain active. That is the intended product contract after
review: `chat-server` supports multiple active sessions for the same user.
Only a same-browser replacement login that carries the previous session cookie
revokes that previous session.

## Results

| Case | Contract | Result | Notes |
| --- | --- | --- | --- |
| R0 | Public live and ready probes return `204` through Caddy | Passed | Confirms public readiness through the reverse proxy. |
| R1 | Unauthenticated WebSocket is rejected | Passed | Browser observed failed WebSocket; server returned authentication failure. |
| R2 | Authenticated `chat.v1` WebSocket opens and sends `ready` | Passed | Session refresh succeeded and `socket.protocol` was `chat.v1`. |
| R3 | Missing subprotocol is rejected | Passed | Server rejected the upgrade. |
| R4 | Conversation creation sends `conversation_created` and is visible over HTTP | Passed | Additional HTTP read evidence was confirmed after the initial run. |
| R5 | Subscription acknowledgement is idempotent | Passed | Repeated subscribe returned `subscribed`. |
| R6 | Same-user fan-out sends `message_posted` to subscribed connections | Passed | Additional HTTP read evidence was confirmed after the initial run. |
| R7 | Unauthorized subscription is rejected without disclosure | Passed | B received `not_found`; additional HTTP non-disclosure checks were confirmed. |
| R8 | Unsubscribe stops future realtime notifications | Passed | HTTP access remained available for the member. |
| R9 | Reconnect recovers missed state through HTTP, not replay | Passed | Missed message was recovered through HTTP history. |
| R10 | Logout closes the matching session socket | Passed | Socket closed with session-ended semantics. |
| R11 | Separate same-user sessions remain independent | Passed | Two profiles for the same user held independent WebSockets. |
| R12 | Protocol violations close the socket | Passed | Invalid JSON and binary data closed as expected. |
| R13 | Per-user connection limit is enforced | Passed | The ninth same-user connection was rejected. |
| R14 | Server shutdown closes WebSockets and drains cleanly | Passed | SIGTERM produced server-shutdown close behavior. |

## Scope Notes

This was a manual production-like E2E run. It is intentionally not treated as
a fully automated conformance suite. Local formatting, check, Clippy, and test
gates were already covered by the repository CI and are not repeated here.

Operational logging policy for Caddy and external log shippers remains an
operator responsibility. The application-level evidence showed path-only
request logging and no application log entries containing cookies, OIDC query
values, CSRF tokens, admission codes, session tokens, or request bodies.

## Conclusion

The realtime E2E gate passed for the current application contract. The system
is ready to proceed toward the browser client and single-binary packaging
phase, while keeping WebSocket notifications as recoverable hints over
SQLite-backed HTTP state.
