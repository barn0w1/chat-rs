# Production-Like E2E Verification Plan

Status: planned; local Rust verification and an explicit Caddy logging decision remain
Date: 2026-06-22

## Purpose

This verification exercises boundaries that unit and in-process integration
tests cannot cover together:

```text
browser
  -> public HTTPS and DNS
  -> Caddy reverse proxy
  -> chat-server HTTP routes
  -> Google OpenID Connect
  -> secure browser cookies
  -> server admission
  -> SQLite durability
  -> authenticated chat use cases
```

The goal is not broad exploratory testing. It is to demonstrate that the
implemented trust, origin, identity, admission, session, and persistence
contracts work in the intended deployment topology.

## Logging Decision Before Real Login

Do not begin the OIDC cases until request logging has been reviewed.

The application trace layer now records only method and Axum's matched route
template, falling back to `request.uri().path()` for unmatched requests. It
does not record the query or request headers. An OIDC callback still carries
`code` and `state` in its query, so Caddy access logging must be checked
independently. The application cannot impose a reverse-proxy logging policy;
that decision belongs to the server operator.

Before E2E:

1. Run the local formatting, check, Clippy, and test gate against the hardened
   code.
2. Inspect the effective Caddy configuration.
3. If Caddy HTTP access logging is enabled, choose whether to skip
   `/auth/oidc/callback`, filter the `code` and `state` query values, or retain
   them under an explicitly accepted access-control and retention policy.
4. Confirm that Cookie, Set-Cookie, Authorization, request bodies, admission
   codes, and OIDC query secrets are not collected by any external log shipper.

Caddy redacts common credential headers by default, but request URIs are normal
access-log fields. Its official log filter can redact query parameters. The
simplest alternative is to skip access logging for the callback path.

For an existing access log, this filter retains the parameter names while
removing their values:

```caddyfile
log {
    format filter {
        request>uri query {
            replace code REDACTED
            replace state REDACTED
        }
    }
}
```

Validate the adapted Caddyfile before reloading it and inspect an actual test
entry. If the deployment already wraps another log encoder, integrate the
query filter into that encoder rather than creating a second conflicting
`format` declaration.

The required E2E gate is an explicit, documented logging decision. Redaction is
the recommended defense-in-depth default, but it is not an application
protocol requirement and cannot be enforced by `chat-server`.

## Deployment Under Test

Use the actual intended topology:

```text
https://chat.hss-science.org
        |
        v
Caddy TLS termination
        |
        v
http://127.0.0.1:3000
        |
        v
chat-server -> dedicated SQLite E2E database
```

The existing readiness response already proves public DNS, TLS, HTTP/2, Caddy,
reverse proxying, and the server readiness route:

```text
HEAD https://chat.hss-science.org/health/ready -> 204
```

The remaining cases focus on browser and application state.

## Isolated Test State

Use a dedicated database, not a database containing valued chat data:

```text
/var/lib/chat-rs/chat-e2e.sqlite3
```

Before starting:

- stop the test instance before replacing or deleting this file;
- preserve any current database separately;
- restrict the database and environment file to the server account;
- use three Google test accounts, labeled A, B, and C; and
- use separate browser profiles or containers for each account.

Three accounts make identity and session transitions unambiguous:

- A: denied first, then admitted with the shared code;
- B: admitted with the same shared code;
- C: admitted later in `open` mode.

If the Google OAuth app is in testing status, add these accounts as test users.
Do not record their email addresses in E2E evidence; the application identity
key is Google's validated subject, not email.

## Google Configuration

Create or use an OAuth client of type Web application.

Required authorized redirect URI:

```text
https://chat.hss-science.org/auth/oidc/callback
```

Configure the issuer exactly as Google publishes it, without a trailing slash:

```sh
CHAT_OIDC_ISSUER=https://accounts.google.com
```

Issuer identifiers are exact strings under OIDC Discovery. They are not URL
origins to normalize.

The redirect URI must match exactly. The backend flow does not require a Google
JavaScript origin because the server redirects directly to Google's
authorization endpoint and exchanges the code itself.

Use only the scopes requested by the implementation (`openid` and `profile`).
No Google API access, refresh token, or offline access is needed.

Store the client secret outside shell history, preferably in a mode-`0600`
environment file readable only by the service account.

## Server Configuration

Initial phase:

```text
CHAT_LISTEN_ADDR=127.0.0.1:3000
CHAT_DATABASE_PATH=/var/lib/chat-rs/chat-e2e.sqlite3
CHAT_PUBLIC_URL=https://chat.hss-science.org
CHAT_ADMISSION_MODE=invite_only
CHAT_OIDC_ISSUER=https://accounts.google.com
CHAT_OIDC_CLIENT_ID=<Google web client ID>
CHAT_OIDC_CLIENT_SECRET=<Google web client secret>
RUST_LOG=chat_server=debug,tower_http=info
```

`CHAT_PUBLIC_URL` is the trusted external origin even though Caddy connects to
the application over loopback HTTP. The server derives the callback URL,
expected Origin, and Secure `__Host-` cookie policy from this value; it does not
trust forwarded Host headers for these decisions.

Expected startup evidence:

```text
configuration accepted ... admission_mode=InviteOnly
SQLite opened and migrated
OIDC provider discovered issuer=https://accounts.google.com
listener bound
server ready
```

Do not include the client secret or raw configuration environment in captured
evidence.

## Browser Test Utilities

The web client does not exist yet. A successful callback redirects to `/`,
which currently returns `404`. That is expected and does not mean login failed.
Verify the session through `/api/v1/session`.

To submit an admission code without placing it in a URL or JavaScript source:

1. Open `https://chat.hss-science.org/`.
2. Open the browser developer console.
3. Run the following snippet.
4. Enter the code only in the prompt.

```js
const form = document.createElement("form");
form.method = "post";
form.action = "/auth/oidc/start";
const input = document.createElement("input");
input.type = "hidden";
input.name = "admission_code";
input.value = prompt("Admission code") ?? "";
form.append(input);
document.body.append(form);
form.submit();
```

This creates a top-level same-origin form POST, allowing the exact Origin check
and the subsequent cross-origin Google redirect to behave like the future web
client.

Clear the developer console after using the code. Never paste it into a URL,
command-line argument, screenshot, issue, or test report.

## Verification Cases

Execute cases in this order against the same E2E database.

### E0: Public readiness

```sh
curl -fsSI https://chat.hss-science.org/health/live
curl -fsSI https://chat.hss-science.org/health/ready
```

Expected: both return `204`; the response is served through Caddy.

### E1: Unknown identity without a code

In browser profile A, navigate to:

```text
https://chat.hss-science.org/auth/oidc/start
```

Complete Google authentication.

Expected:

- Google authentication and ID-token verification complete;
- callback returns `401 application/problem+json` with `login-failed`;
- no `__Host-chat_session` cookie exists;
- `GET /api/v1/session` returns `401`; and
- no `users` or `auth_identities` row was created for A.

This proves that authentication is permitted but does not imply admission.

### E2: Invalid admission code

From a page at the chat origin, submit the form utility with an arbitrary
invalid value.

Expected:

- `POST /auth/oidc/start` returns `403`;
- the browser is not redirected to Google; and
- no OIDC transaction or local user is created.

Also verify Origin rejection without using a real code:

```sh
curl -i -X POST https://chat.hss-science.org/auth/oidc/start \
  -H 'Origin: https://invalid.example' \
  -H 'Content-Type: application/x-www-form-urlencoded' \
  --data 'admission_code=invalid'
```

Expected: `403`.

### E3: Create a code while the server is running

Run the release binary as a second process using the exact same database path:

```sh
CHAT_DATABASE_PATH=/var/lib/chat-rs/chat-e2e.sqlite3 \
/path/to/chat-server admission-code create --valid-for-hours 2
```

Expected:

- command succeeds without stopping or restarting the server;
- output contains one code and `expires_at_ms`;
- health readiness remains `204`; and
- application logs contain no raw code.

Keep the code only for E4 and E5, then discard it.

### E4: Admit account A

In browser profile A, use the form utility with the active code and complete
Google authentication.

Expected:

- OIDC callback returns `303` to `/`;
- following `/` may show the expected `404`;
- `GET /api/v1/session` returns `200` with User A and a CSRF token;
- one user and one identity binding exist; and
- the response sets `__Host-chat_session` and clears `__Host-chat_login`.

Inspect the session cookie in browser developer tools:

```text
name: __Host-chat_session
Secure: true
HttpOnly: true
SameSite: Lax
Path: /
Domain attribute: absent
```

Do not copy the cookie value into evidence.

### E5: Reuse the code for account B

In separate browser profile B, submit the same code and authenticate as B.

Expected:

- B receives a session;
- B has a different local string-encoded User ID from A;
- A's session remains valid in profile A; and
- the shared admission-code row remains available until expiry.

This is the defining operational behavior of reusable admission codes.

### E6: Existing identity without a code

Log A out, or remove only A's chat session cookie. Then use the ordinary GET
login route without an admission code.

Expected:

- A receives a new session in `invite_only` mode;
- A resolves to the same local User ID; and
- no additional user or identity binding is created.

Restart the server against the same E2E database and repeat this case once to
verify persistence across process lifecycle.

### E7: Session and CSRF behavior

From profile A, run this in the chat-origin developer console:

```js
(async () => {
  const sessionResponse = await fetch("/api/v1/session", { cache: "no-store" });
  const session = await sessionResponse.json();
  const denied = await fetch("/api/v1/conversations", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ title: "Denied" }),
  });
  console.log({ sessionStatus: sessionResponse.status, deniedStatus: denied.status });
  window.chatE2E = { csrf: session.csrf_token };
})();
```

Expected: session `200`; mutation without CSRF `403`.

### E8: Authenticated chat workflow

Continue in profile A:

```js
(async () => {
  const created = await fetch("/api/v1/conversations", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "X-CSRF-Token": window.chatE2E.csrf,
    },
    body: JSON.stringify({ title: "E2E" }),
  });
  const conversation = await created.json();
  const posted = await fetch(`/api/v1/conversations/${conversation.id}/messages`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "X-CSRF-Token": window.chatE2E.csrf,
    },
    body: JSON.stringify({ body: "hello from e2e" }),
  });
  const message = await posted.json();
  const history = await fetch(
    `/api/v1/conversations/${conversation.id}/messages?limit=10`,
    { cache: "no-store" },
  );
  console.log({
    createStatus: created.status,
    postStatus: posted.status,
    historyStatus: history.status,
    conversation,
    message,
    history: await history.json(),
  });
})();
```

Expected: create `201`, post `201`, history `200`, and the persisted message is
returned. IDs must be JSON strings.

### E9: Logout

```js
(async () => {
  const response = await fetch("/api/v1/session", {
    method: "DELETE",
    headers: { "X-CSRF-Token": window.chatE2E.csrf },
  });
  console.log(response.status);
})();
```

Expected: `204`, session cookie removed, subsequent session request `401`.

### E10: Open admission

Restart the server with the same E2E database and:

```text
CHAT_ADMISSION_MODE=open
```

In browser profile C, use ordinary GET login without a code.

Expected: C is admitted and receives a session. Then restore
`CHAT_ADMISSION_MODE=invite_only` and restart. A, B, and C must remain able to
log in without a code because admission policy applies only to new identities.

## Expiry Coverage

The public command intentionally has a minimum lifetime of one hour. Waiting an
hour or altering the server clock/database does not add enough E2E value to
justify the operational risk.

Expiry-at-the-boundary, expiry during OIDC, and existing-user access after
expiry are covered by deterministic store tests with injected `SystemTime`.
For E2E, record the command's `expires_at_ms` and confirm only that the code is
accepted before that instant. A natural expiry can be checked later without
blocking this acceptance run.

## Log Review

After all cases, inspect both application and Caddy logs.

Allowed evidence includes:

- method;
- path without query;
- status;
- latency;
- configured issuer; and
- non-secret lifecycle messages.

The following must never appear in application logs:

- Google authorization `code`;
- OIDC `state` or nonce;
- PKCE verifier;
- client secret;
- raw admission code;
- raw session or CSRF token;
- Cookie or Set-Cookie values; or
- ID-token claims beyond deliberately logged non-secret configuration.

Apply the same list to Caddy when the operator selected redaction or omission.
If callback query retention was deliberately selected, verify that its access,
shipping, backup, and retention behavior matches the accepted policy. An
authorization code is short-lived, single-use, and protected here by exact
redirect URI, confidential-client authentication, server-held PKCE, state, and
browser binding. Logging it still broadens exposure, enables correlation and
race attempts, and increases the impact of a combined log/server compromise.

Search by field names, but do not paste known secret values into shell history
for `grep`. If any secret was logged, stop testing, restrict the log files,
rotate the affected ephemeral values, fix logging, and begin with a fresh E2E
database.

## Evidence Record

Record one row per case:

| Case | Time | Account label | Expected | Actual status | Result | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| E0 | | none | live/ready 204 | | | |
| E1 | | A | verified but denied | | | |
| E2 | | none | invalid code/origin 403 | | | |
| E3 | | none | live code creation | | | |
| E4 | | A | admitted/session 200 | | | |
| E5 | | B | same code admitted | | | |
| E6 | | A | existing login, same ID | | | |
| E7 | | A | CSRF denial | | | |
| E8 | | A | create/post/read | | | |
| E9 | | A | logout | | | |
| E10 | | C | open admission | | | |

Evidence must redact all bearer values and personal account identifiers.

## Completion Criteria

The E2E gate passes when:

- request-query secrets are absent from application logs;
- Caddy callback-query handling matches the operator's documented logging
  decision;
- Google discovery, redirect, callback, and ID-token verification succeed;
- Secure host-only cookies work through Caddy TLS termination;
- authentication without admission creates no unknown user in `invite_only`;
- one live code admits A and B without a server restart;
- existing identities log in without codes across a server restart;
- `open` admits C without a code;
- session, CSRF, logout, and chat HTTP workflows behave as documented;
- SQLite retains the expected users, bindings, conversation, and message; and
- no unexpected `5xx`, panic, or secret-bearing log entry occurs.

After completion, stop the E2E instance or return it to the intended mode,
remove the dedicated E2E database when evidence is no longer required, discard
the admission code, and remove unnecessary Google test-user access.

## Sources

- [Google OpenID Connect](https://developers.google.com/identity/openid-connect/openid-connect)
- [Google OAuth production readiness](https://developers.google.com/identity/protocols/oauth2/production-readiness/policy-compliance)
- [Caddy reverse_proxy](https://caddyserver.com/docs/caddyfile/directives/reverse_proxy)
- [Caddy access-log filtering](https://caddyserver.com/docs/caddyfile/directives/log)
- [tower-http DefaultMakeSpan](https://docs.rs/tower-http/latest/tower_http/trace/struct.DefaultMakeSpan.html)
- [MDN Set-Cookie](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Set-Cookie)
