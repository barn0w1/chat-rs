# Real-Time E2E Verification

Status: completed; application-level cases passed
Date: 2026-06-23
Baseline: `58ba53e`

The completed evidence summary is recorded in
[`realtime-e2e-verification-report-2026-06-23.md`](realtime-e2e-verification-report-2026-06-23.md).

## Purpose

This verification checks the real-time boundary through the intended
deployment topology:

```text
browser
  -> public HTTPS and DNS
  -> Caddy reverse proxy
  -> chat-server HTTP and WebSocket routes
  -> server-side session cookies
  -> SQLite-backed chat state
```

The goal is not to prove durable event delivery. The implemented contract is
that SQLite and authenticated HTTP resources are authoritative, while
WebSocket messages are small change notifications that tell the browser to
resynchronize over HTTP.

The E2E run must therefore verify both parts:

- the browser can open, use, and close the authenticated WebSocket channel
  through Caddy; and
- every notification can be reconciled through the existing HTTP API.

## Source Notes

This plan follows the browser and server contracts used by the current
implementation:

- Browser WebSocket construction accepts a URL and optional protocols; the
  selected subprotocol is available through `WebSocket.protocol`.
- A failed browser WebSocket handshake is observed as an error followed by a
  close event rather than as a readable HTTP response body.
- Browser close events expose `code`, `reason`, and `wasClean`.
- Axum's `WebSocketUpgrade::protocols` selects and echoes a supported
  subprotocol, and `selected_protocol` is used by the handler before accepting
  the upgrade.
- Caddy `reverse_proxy` is the public-to-loopback hop and must pass the
  upgrade route to `chat-server`.

Primary references:

- <https://developer.mozilla.org/en-US/docs/Web/API/WebSocket/WebSocket>
- <https://developer.mozilla.org/en-US/docs/Web/API/WebSocket/close_event>
- <https://docs.rs/axum/0.8.9/axum/extract/struct.WebSocketUpgrade.html>
- <https://caddyserver.com/docs/caddyfile/directives/reverse_proxy>
- <https://datatracker.ietf.org/doc/html/rfc6455>

## Deployment Under Test

Use the production-like domain:

```text
https://chat.hss-science.org
```

The expected upstream remains:

```text
http://127.0.0.1:3000
```

Use a dedicated disposable database for this run. Do not reuse a valued chat
database.

```text
CHAT_LISTEN_ADDR=127.0.0.1:3000
CHAT_DATABASE_PATH=/var/lib/chat-rs/chat-realtime-e2e.sqlite3
CHAT_PUBLIC_URL=https://chat.hss-science.org
CHAT_ADMISSION_MODE=invite_only
CHAT_OIDC_ISSUER=https://accounts.google.com
CHAT_OIDC_CLIENT_ID=<Google web client ID>
CHAT_OIDC_CLIENT_SECRET=<Google web client secret>
RUST_LOG=chat_server=debug,tower_http=info
```

Expected startup evidence:

```text
configuration accepted ... admission_mode=InviteOnly
SQLite opened and migrated
OIDC provider discovered issuer=https://accounts.google.com
listener bound
server ready
```

Do not capture raw environment files, client secrets, cookies, CSRF tokens,
OIDC query values, admission codes, or provider subjects in the final report.

## Temporary Browser Harness

The repository includes a test-only browser page:

```text
web/e2e/realtime.html
```

Serve this page from the same public origin as the server, for example:

```text
https://chat.hss-science.org/_e2e/realtime
```

Do not open the file with a `file://` URL. The server intentionally requires
the request Origin to match `CHAT_PUBLIC_URL`, and a file or developer-console
navigation can produce an opaque or `null` Origin.

One Caddy pattern is:

```caddyfile
chat.hss-science.org {
    handle /_e2e/realtime {
        root * /srv/chat-rs/web/e2e
        rewrite * /realtime.html
        header {
            Cache-Control no-store
            X-Content-Type-Options nosniff
        }
        file_server
    }

    handle {
        reverse_proxy 127.0.0.1:3000
    }
}
```

Adapt the path to wherever the checked-out repository or copied harness file
lives on the server. Validate and reload Caddy before testing.

Remove this route after the E2E run. The harness is not a production web
client and should not remain publicly exposed.

## Logging Gate

Before real login:

1. Confirm the application logs method, matched path, status, and latency
   without query strings or request headers.
2. Confirm Caddy access logging policy for `/auth/oidc/callback`.
3. Confirm no external log shipper collects Cookie, Set-Cookie,
   Authorization, request bodies, admission codes, OIDC `code`, OIDC `state`,
   session tokens, CSRF tokens, or WebSocket payloads.

The application cannot force Caddy's logging policy. The operator must decide
whether to skip, redact, retain, or access-control proxy logs.

## Browser Profiles

Use separate clean browser profiles or private windows:

- A: main admitted user and conversation owner.
- B: second admitted user for unauthorized subscription checks.

Two A windows are useful for same-user fan-out:

- A1: creates and posts.
- A2: holds another WebSocket connection for the same session.

Current public HTTP routes do not include add-member or remove-member
workflows. Cross-user positive fan-out is therefore out of scope for this run.
Use two A connections for positive fan-out and B for non-disclosure checks.

## Harness Operation Model

The harness keeps CSRF tokens and test IDs only in browser memory. It does not
store secrets in local storage.

Important controls:

- `Refresh session`: calls `GET /api/v1/session` and stores CSRF in memory.
- `Login with code`: posts an admission code to `/auth/oidc/start`.
- `Login existing`: redirects to `/auth/oidc/start`.
- `Connect`: opens `new WebSocket("/api/v1/ws", "chat.v1")`.
- `Connect without protocol`: negative test for mandatory subprotocol.
- `Create conversation`: HTTP mutation with CSRF.
- `Subscribe`: sends `{"type":"subscribe","conversation_id":"..."}`.
- `Post message`: HTTP mutation with CSRF.
- `Fetch latest messages`: HTTP read for authoritative state.
- `Unsubscribe`: sends `{"type":"unsubscribe","conversation_id":"..."}`.
- `Send invalid JSON`: negative protocol test.
- `Send binary`: negative unsupported-data test.
- `Logout`: deletes the session and should close matching WebSockets.
- `Export log`: downloads the in-browser event log as JSON.

The browser's `WebSocket` API does not expose Ping/Pong frames, so heartbeat
is verified indirectly through server logs and through connection survival
during normal cases. If a zombie-client test is needed later, use a separate
non-browser client.

## Verification Cases

Execute cases in order unless a case explicitly states that it can be run
independently.

### R0: Public Readiness

Run:

```sh
curl -fsSI https://chat.hss-science.org/health/live
curl -fsSI https://chat.hss-science.org/health/ready
```

Expected:

- both return `204`;
- responses pass through Caddy; and
- server logs record only safe path-only request data.

### R1: Unauthenticated WebSocket Is Rejected

Use a clean profile with no session cookie.

Steps:

1. Open `/_e2e/realtime`.
2. Click `Connect`.

Expected:

- the WebSocket does not open;
- the harness records `error` and `close`;
- server logs show `GET /api/v1/ws` with an authentication failure status;
  and
- no user, session, or conversation state is created.

Because browser WebSocket failures do not expose the HTTP response body, use
server logs for the exact status.

### R2: Login and Authenticated WebSocket

Steps:

1. Create an admission code with the operator command.
2. In profile A, open `/_e2e/realtime`.
3. Enter the code and click `Login with code`.
4. Complete Google login.
5. Return to `/_e2e/realtime`.
6. Click `Refresh session`.
7. Click `Connect`.

Expected:

- session endpoint returns `200`;
- the WebSocket opens;
- `socket.protocol` is `chat.v1`;
- the first application message is `{"type":"ready"}`; and
- logs do not contain cookies, CSRF, OIDC values, or admission code material.

### R3: Mandatory Subprotocol

Steps:

1. In profile A with a valid session, click `Connect without protocol`.

Expected:

- the connection does not open;
- the harness records `error` and `close`;
- server logs show a rejected `/api/v1/ws` request; and
- the normal `Connect` button still succeeds afterward.

### R4: Conversation-Created Notification

Steps:

1. In profile A, keep a `chat.v1` WebSocket open.
2. Click `Create conversation`.

Expected:

- HTTP returns `201`;
- the harness stores the returned `conversation_id`;
- the socket receives
  `{"type":"conversation_created","conversation_id":"..."}`; and
- `GET /api/v1/conversations` includes the same conversation ID.

The HTTP mutation response is authoritative. The notification only confirms
that another tab could refresh.

### R5: Subscribe Success

Steps:

1. Use the conversation ID from R4.
2. Click `Subscribe`.

Expected:

- the socket receives
  `{"type":"subscribed","conversation_id":"..."}`; and
- repeating `Subscribe` returns another successful acknowledgement without
  creating a second logical subscription.

### R6: Same-User Fan-Out

Steps:

1. Open profile A in two windows, A1 and A2, or open the harness twice in the
   same profile.
2. In both windows, click `Refresh session`.
3. In both windows, click `Connect`.
4. In both windows, set the same conversation ID and click `Subscribe`.
5. In A1, click `Post message`.

Expected:

- the HTTP post returns `201`;
- both A1 and A2 receive `message_posted` for the same `conversation_id` and
  `message_id`;
- `Fetch one message` or `Fetch latest messages` returns the posted body over
  HTTP; and
- duplicate notifications are treated as harmless by ID.

### R7: Unauthorized Subscribe Is Non-Disclosing

Steps:

1. Admit profile B with the same active admission code, or use an already
   admitted B.
2. In B, open `/_e2e/realtime`, refresh session, and connect.
3. Enter A's conversation ID.
4. Click `Subscribe`.

Expected:

- the WebSocket stays open;
- B receives
  `{"type":"subscription_rejected","conversation_id":"...","reason":"not_found"}`;
- B cannot fetch the conversation or its messages over HTTP; and
- the response does not distinguish absent conversation from lack of
  membership.

### R8: Unsubscribe Stops Future Notifications

Steps:

1. In A2, while subscribed to A's conversation, click `Unsubscribe`.
2. In A1, click `Post message`.

Expected:

- A2 receives `unsubscribed`;
- A2 does not receive the subsequent `message_posted`;
- A1 or another subscribed A connection still receives the notification; and
- A2 can still fetch history over HTTP because it remains a conversation
  member.

### R9: Reconnect and HTTP Recovery

Steps:

1. In A2, subscribe and confirm message notifications work.
2. Close A2's socket with `Close socket`.
3. In A1, post a message while A2 is disconnected.
4. In A2, reconnect and subscribe again.
5. In A2, click `Fetch latest messages`.

Expected:

- A2 does not receive replayed WebSocket notifications for messages posted
  while disconnected;
- `Fetch latest messages` returns the missed message; and
- this confirms the intended recovery model.

### R10: Logout Closes Matching Session

Steps:

1. In A, keep a socket open.
2. Click `Logout`.

Expected:

- `DELETE /api/v1/session` returns `204`;
- the socket closes;
- close code is expected to be `1008`;
- close reason is expected to be `session ended`; and
- a later `Refresh session` returns `401`.

### R11: Same-User Multi-Session Behavior

Steps:

1. In A1, login and connect.
2. In a second A browser profile, login again as the same Google account.
3. Refresh session and connect in A2.
4. Observe A1's old socket.

Expected:

- both profiles resolve to the same user ID;
- both profiles can hold independent sessions and WebSockets; and
- A1's socket is not closed merely because A2 logged in.

The server supports multiple active sessions for the same user so one account
can be used from multiple browsers or devices. A previous session is revoked
only when the login request carries that previous session cookie and replaces
it in the same browser context.

### R12: Protocol Violations

Run these independently because each should close the socket.

Invalid JSON:

1. Connect.
2. Click `Send invalid JSON`.

Expected:

- socket closes with policy-violation semantics, expected code `1008`.

Binary:

1. Connect.
2. Click `Send binary`.

Expected:

- socket closes with unsupported-data semantics, expected code `1003`.

Do not paste arbitrary secret-bearing data into these controls.

### R13: Per-User Connection Limit

Steps:

1. Close the normal harness socket if it is open.
2. Close any leftover extra sockets.
3. In the same authenticated profile, click `Open 9 sockets`.

Expected:

- up to eight sockets open;
- the ninth does not open or is rejected by the server;
- server logs show a capacity rejection without panic; and
- closing all sockets releases capacity.

The current default limit is eight connections per user.

### R14: Server Shutdown Closes WebSockets

Steps:

1. Keep one or more authenticated sockets open.
2. Send SIGTERM to the server process.

Expected:

- sockets close with server-shutdown semantics, expected code `1012` and
  reason `server shutdown`;
- server logs show graceful WebSocket drain followed by HTTP serving stopped
  and SQLite pool closed; and
- no shutdown timeout occurs.

Restart the server after this case if more tests are needed.

## Evidence Template

Record a redacted report with this structure:

```markdown
# Realtime E2E Verification Report

Status:
Date:
Target commit:
Target binary:
Database:
Public origin:

## Preconditions

- Local fmt/check/clippy/test:
- Caddy route installed:
- Logging policy reviewed:
- Browser profiles:

## Results

| Case | Expected | Actual | Result | Notes |
| --- | --- | --- | --- | --- |
| R0 | live/ready 204 | | | |
| R1 | unauthenticated ws rejected | | | |
| R2 | authenticated chat.v1 ready | | | |
| R3 | missing protocol rejected | | | |
| R4 | conversation_created | | | |
| R5 | subscribed | | | |
| R6 | same-user message fan-out | | | |
| R7 | unauthorized subscribe rejected as not_found | | | |
| R8 | unsubscribe stops notifications | | | |
| R9 | reconnect recovers via HTTP | | | |
| R10 | logout closes session socket | | | |
| R11 | same-user multi-session behavior | | | |
| R12 | protocol violations close | | | |
| R13 | per-user connection limit enforced | | | |
| R14 | shutdown closes sockets | | | |

## Log Review

- Application logs include no query strings:
- Application logs include no Cookie or Set-Cookie values:
- Caddy callback query policy:
- No unexpected 5xx, panic, or shutdown timeout:

## Final State

- users:
- conversations:
- messages:
- auth_sessions:
- oidc_login_transactions:

## Cleanup

- Removed temporary Caddy route:
- Removed or archived harness file:
- Restored admission mode:
- Deleted or retained disposable DB:
```

Do not paste exported harness logs without reviewing them first. The harness
does not intentionally record secrets, but evidence should still be reviewed
before publishing or committing.

## Completion Criteria

The realtime E2E gate passes when:

1. authenticated WebSocket upgrade works through Caddy;
2. `chat.v1` subprotocol negotiation is enforced;
3. `ready`, `subscribed`, `unsubscribed`, and expected rejection messages are
   observed;
4. committed HTTP mutations produce expected notifications;
5. notifications are reconciled through HTTP reads;
6. reconnect does not imply replay and HTTP recovery works;
7. logout and shutdown close sockets intentionally, while separate same-user
   sessions remain independent;
8. protocol violations close the socket without server panic;
9. connection limits are enforced; and
10. no secrets, query values, or request bodies appear in application logs.
