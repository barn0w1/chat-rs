# Web Client Initial Implementation Plan

Status: Step 1 implemented; later steps not started  
Date: 2026-06-24  
Baseline: `c7a3028`  
Depends on: [`web-client-plan.md`](web-client-plan.md)

## Purpose

This document turns the approved web-client direction into the first concrete
implementation plan for Milestone 6.

The stack decision is now treated as accepted for the first production client:

- TypeScript
- Vite
- Lit and standard custom elements
- platform Fetch and WebSocket APIs
- plain CSS with explicit design tokens

The client should feel like the rest of `chat-rs`: small, fast, durable,
explicit about failure, and conservative about security. The first goal is not
to build a broad social product. The first goal is to replace the Vite starter
with a complete, production-shaped browser client for the server contract that
already exists.

## Current Baseline

Commit `c7a3028` added a Vite + TypeScript + Lit app under `web/` and confirmed
that it builds in the user's development environment.

The current `web/` directory is still starter content:

- `web/index.html` loads `/src/my-element.ts`;
- `web/src/my-element.ts` renders the Vite/Lit starter UI;
- `web/src/index.css` contains starter global layout;
- runtime dependencies are currently limited to `lit`;
- scripts are `dev`, `build`, and `preview`;
- there is no `vite.config.ts` yet.

This is a good Phase 0 result. The first implementation should now remove the
starter UI and establish the production app shape.

Implementation update: Step 1 has replaced the starter UI with a static
`chat-app` shell, added base/layout/token CSS, removed starter assets, and kept
HTTP and WebSocket behavior out of scope for this step.

## Source Contracts the Client Must Preserve

### HTTP Truth

Authenticated HTTP resources are the durable source of truth.

Implemented routes the client should use:

| Method | Path | Client use |
| --- | --- | --- |
| `GET` | `/api/v1/session` | load authenticated user and CSRF token |
| `DELETE` | `/api/v1/session` | logout |
| `GET` | `/auth/oidc/start` | begin normal OIDC login |
| `POST` | `/auth/oidc/start` | begin OIDC login with admission code form |
| `GET` | `/api/v1/conversations?before=&limit=` | list visible conversations |
| `POST` | `/api/v1/conversations` | create conversation |
| `GET` | `/api/v1/conversations/{conversation_id}` | read one conversation |
| `GET` | `/api/v1/conversations/{conversation_id}/members?after=&limit=` | list members if needed |
| `GET` | `/api/v1/conversations/{conversation_id}/messages?before=&limit=` | list messages |
| `POST` | `/api/v1/conversations/{conversation_id}/messages` | post message |
| `GET` | `/api/v1/conversations/{conversation_id}/messages/{message_id}` | fetch one message |

All wire IDs are decimal strings. The client must not coerce IDs to `number`.

Unsafe requests require same-origin browser credentials, `Origin`, and the
session CSRF token returned by `/api/v1/session`. The client should keep the
CSRF token in memory only.

Until HTTP mutation idempotency exists, `POST` requests must not be
automatically retried after dispatch. If a network failure occurs after the
request was sent, the UI should show an unknown-result state and offer a manual
refresh path.

### Realtime Hints

The WebSocket route is:

```text
GET /api/v1/ws
Sec-WebSocket-Protocol: chat.v1
```

The client must open it as a same-origin socket and request the exact
`chat.v1` subprotocol. WebSocket notifications are hints only. They do not
carry message bodies and they do not provide replay.

Supported client messages:

```json
{"type":"subscribe","conversation_id":"42"}
{"type":"unsubscribe","conversation_id":"42"}
```

Supported server messages:

```json
{"type":"ready"}
{"type":"subscribed","conversation_id":"42"}
{"type":"unsubscribed","conversation_id":"42"}
{"type":"subscription_rejected","conversation_id":"42","reason":"not_found"}
{"type":"conversation_created","conversation_id":"42"}
{"type":"message_posted","conversation_id":"42","message_id":"99"}
```

The recovery rule is simple: after connect, reconnect, tab focus, or any
ambiguous realtime condition, fetch current state over HTTP and merge by ID.

## External References

The direction is based on primary documentation:

- MDN describes Web Components as reusable custom elements with encapsulated
  functionality, built from custom elements, Shadow DOM, and templates:
  <https://developer.mozilla.org/en-US/docs/Web/API/Web_components>
- Lit components are standard custom elements with scoped styles, reactive
  properties, and declarative templates:
  <https://lit.dev/>
- Lit works with the official TypeScript compiler and build tools such as
  Vite:
  <https://lit.dev/docs/tools/development/>
- Vite production builds use `index.html` as an entry point and output static
  assets suitable for static serving:
  <https://vite.dev/guide/build>
- Vite is close to a static file server during development while adding modern
  ESM, dependency, and HMR support:
  <https://vite.dev/guide/features>
- MDN documents `new WebSocket(url, protocols)` and subprotocol negotiation:
  <https://developer.mozilla.org/en-US/docs/Web/API/WebSocket/WebSocket>
- MDN documents Fetch as the modern promise-based HTTP API:
  <https://developer.mozilla.org/en-US/docs/Web/API/Fetch_API/Using_Fetch>
- MDN documents same-origin as the default Fetch credentials mode:
  <https://developer.mozilla.org/en-US/docs/Web/API/Request/credentials>
- Core Web Vitals define practical loading, interaction, and stability targets:
  <https://web.dev/articles/vitals>

## Implementation Principles

1. Replace starter code before adding product behavior.
2. Keep the runtime dependency list at `lit` for the first implementation.
3. Model server representations explicitly in TypeScript.
4. Keep browser state in small stores and controllers, not a global framework.
5. Use DOM events for component output.
6. Keep all private chat content in memory for the first client.
7. Do not add a service worker, IndexedDB, or localStorage cache.
8. Do not invent client-side authentication tokens.
9. Treat HTTP reads and mutation responses as authoritative.
10. Treat WebSocket notifications as invalidation hints.
11. Prefer visible, honest failure states over hidden retries.
12. Keep the first UI restrained, responsive, keyboard-accessible, and fast.

## Proposed Directory Layout

The first implementation should move from the starter file layout to this
shape:

```text
web/
|-- index.html
|-- package.json
|-- pnpm-lock.yaml
|-- tsconfig.json
|-- vite.config.ts
|-- public/
|   `-- favicon.svg
`-- src/
    |-- main.ts
    |-- app-shell.ts
    |-- api/
    |   |-- client.ts
    |   |-- problems.ts
    |   `-- types.ts
    |-- realtime/
    |   |-- protocol.ts
    |   `-- socket.ts
    |-- state/
    |   |-- conversation-store.ts
    |   |-- message-store.ts
    |   `-- session-store.ts
    |-- components/
    |   |-- chat-login.ts
    |   |-- chat-layout.ts
    |   |-- connection-status.ts
    |   |-- conversation-list.ts
    |   |-- message-composer.ts
    |   `-- message-list.ts
    `-- styles/
        |-- base.css
        |-- layout.css
        `-- tokens.css
```

`web/src/my-element.ts` and starter-only assets should be deleted when the
production shell is introduced.

## Data Model

Create transport DTOs in `src/api/types.ts`. These should mirror the server's
JSON boundary exactly.

```ts
export type Id = string;

export interface User {
  id: Id;
  display_name: string;
  created_at_ms: number;
}

export interface Session {
  user: User;
  csrf_token: string;
}

export interface Conversation {
  id: Id;
  title: string;
  created_at_ms: number;
  role: 'owner' | 'member';
}

export interface ConversationPage {
  conversations: Conversation[];
  next_cursor: Id | null;
}

export interface Message {
  id: Id;
  conversation_id: Id;
  author_id: Id;
  body: string;
  created_at_ms: number;
}

export interface MessagePage {
  messages: Message[];
  next_cursor: Id | null;
}
```

Do not parse ID strings into JavaScript numbers. Timestamps may remain numbers
because the server emits millisecond Unix timestamps within JavaScript's safe
integer range for realistic application dates.

## API Client Plan

`src/api/client.ts` should expose a small typed client:

```text
getSession()
logout(csrfToken)
listConversations({ before, limit })
createConversation({ title }, csrfToken)
getConversation(id)
listMessages(conversationId, { before, limit })
postMessage(conversationId, { body }, csrfToken)
getMessage(conversationId, messageId)
```

Rules:

- use relative URLs;
- use standard same-origin browser credentials;
- set `Accept: application/json` for JSON reads;
- set `Content-Type: application/json` for JSON POSTs;
- set `X-CSRF-Token` only for unsafe requests;
- reject non-JSON success responses as internal client errors;
- parse RFC 9457-style problem responses into a finite client shape;
- preserve raw status and problem fields for display/debugging;
- support `AbortSignal` for stale view loads;
- never log request bodies, admission codes, CSRF tokens, or cookies.

Problem handling should classify at least:

| Status | Client category |
| ---: | --- |
| `400` | invalid request |
| `401` | unauthenticated |
| `403` | forbidden |
| `404` | not found |
| `413` | content too large |
| `415` | unsupported media type |
| `422` | validation |
| `503` | temporarily unavailable |
| other | unexpected |

## State Plan

### SessionStore

Responsibilities:

- load `/api/v1/session` on app start;
- hold `user`, `csrfToken`, `loading`, and `error`;
- expose `authenticated`, `unauthenticated`, and `unknown` states;
- clear all stores on logout or unauthenticated response;
- never persist CSRF or session state to browser storage.

### ConversationStore

Responsibilities:

- hold visible conversations by ID;
- preserve server ordering for the loaded page;
- hold `nextCursor`;
- hold selected conversation ID;
- merge created or fetched conversations by ID;
- expose loading, empty, and error states.

### MessageStore

Responsibilities:

- hold messages by conversation ID and message ID;
- maintain a newest-first or oldest-first view order explicitly;
- merge pages and individual messages by ID;
- track `nextCursor` for older history;
- track pending sends;
- represent unknown mutation result without silently retrying.

### RealtimeCoordinator

Responsibilities:

- open `new WebSocket("/api/v1/ws", "chat.v1")`;
- wait for `ready`;
- subscribe to the selected conversation;
- after `subscribed`, fetch the latest message page;
- on `message_posted`, fetch the individual message by ID;
- on `conversation_created`, refresh or merge the conversation list;
- reconnect with bounded exponential backoff;
- stop on logout;
- surface connection state to the UI.

## Component Plan

### `chat-app`

Root component in `app-shell.ts`.

Responsibilities:

- create stores and coordinator;
- load session on connect;
- switch between login and authenticated layouts;
- wire top-level events;
- perform cleanup in `disconnectedCallback`.

### `chat-login`

Responsibilities:

- show unauthenticated state;
- provide normal OIDC login link/button;
- provide admission-code form using `POST /auth/oidc/start`;
- submit with `application/x-www-form-urlencoded`;
- avoid placing admission codes in URLs;
- show forbidden/unavailable login-start failures.

### `chat-layout`

Responsibilities:

- arrange desktop two-column layout;
- arrange mobile list/detail navigation;
- expose session summary, logout, and connection status;
- keep the first screen as the usable app.

### `conversation-list`

Responsibilities:

- render conversations as a semantic list;
- support keyboard selection;
- show loading, empty, and error states;
- expose create-conversation command;
- provide "load older" when `next_cursor` exists.

### `message-list`

Responsibilities:

- render message history for the selected conversation;
- preserve scroll position when older messages are loaded;
- dedupe by message ID;
- avoid treating realtime order as durable order;
- use a bounded polite announcement for new messages.

### `message-composer`

Responsibilities:

- use a real form and textarea;
- submit with Enter where appropriate and provide an accessible multiline path;
- disable while one send is in flight;
- show pending, sent, validation, and unknown-result states;
- restore focus after successful send;
- never automatically retry a POST.

### `connection-status`

Responsibilities:

- show connected, connecting, reconnecting, disconnected, and unavailable;
- keep the display compact;
- avoid noisy announcements for normal reconnect churn.

## First Implementation Sequence

### Step 1: Replace Starter Shell

Files:

- add `src/main.ts`;
- add `src/app-shell.ts`;
- add `src/styles/*`;
- update `index.html`;
- remove `src/my-element.ts` and starter-only imports/assets.

Behavior:

- render a static app shell;
- no server calls yet;
- prove Lit wiring, CSS tokens, and responsive layout foundation.

Acceptance:

- production build still succeeds in the user's environment;
- no starter UI remains;
- initial JS and CSS output are small and understandable.

### Step 2: API Types, Problems, and Client

Files:

- add `src/api/types.ts`;
- add `src/api/problems.ts`;
- add `src/api/client.ts`.

Behavior:

- implement typed wrappers for the existing HTTP API;
- include abort support for reads;
- implement finite problem classification;
- keep IDs as strings.

Acceptance:

- pure unit tests can cover URL construction, problem classification, and ID
  handling once a test runner is selected;
- code can be manually exercised from the app shell without leaking secrets.

### Step 3: Session and Login Surface

Files:

- add `src/state/session-store.ts`;
- add `src/components/chat-login.ts`;
- extend `chat-app`.

Behavior:

- load `/api/v1/session` on startup;
- show login when unauthenticated;
- show authenticated shell when session exists;
- support admission-code login start;
- support logout using CSRF.

Acceptance:

- authenticated and unauthenticated states are visible;
- CSRF token stays in memory only;
- logout clears stores and closes realtime when realtime exists later.

### Step 4: HTTP Chat MVP

Files:

- add `src/state/conversation-store.ts`;
- add `src/state/message-store.ts`;
- add `src/components/chat-layout.ts`;
- add `src/components/conversation-list.ts`;
- add `src/components/message-list.ts`;
- add `src/components/message-composer.ts`.

Behavior:

- list conversations;
- create conversation;
- select conversation;
- list latest messages;
- load older messages;
- post message;
- fetch one message if needed;
- handle loading, empty, validation, not-found, unauthenticated, and
  unavailable states.

Acceptance:

- the current authenticated HTTP chat workflow no longer requires dev-console
  scripts;
- no automatic POST retry exists;
- duplicate message and conversation merges are harmless.

### Step 5: Realtime Coordinator

Files:

- add `src/realtime/protocol.ts`;
- add `src/realtime/socket.ts`;
- add `src/components/connection-status.ts`;
- extend stores and app shell.

Behavior:

- open `/api/v1/ws` with `chat.v1`;
- parse only known server message shapes;
- send subscribe/unsubscribe for the selected conversation;
- fetch newest messages after `subscribed`;
- fetch individual messages on `message_posted`;
- refresh conversation list on `conversation_created`;
- reconnect with backoff and resubscribe;
- stop on logout or unauthenticated session loss.

Acceptance:

- production client covers the manual realtime E2E happy path currently covered
  by the temporary harness;
- reconnect always rebuilds state from HTTP;
- invalid or unknown protocol messages do not corrupt state.

### Step 6: Polish Gate

Files:

- refine component CSS and interaction code;
- add focused tests if test tooling is introduced in this increment.

Behavior:

- desktop two-column layout;
- mobile list/detail flow;
- keyboard-operable conversation list and composer;
- scroll anchoring for older messages;
- visible focus states;
- reduced-motion support;
- restrained empty, error, connecting, and disconnected states.

Acceptance:

- app is usable on desktop and mobile;
- message send and conversation switch feel immediate;
- no layout shift from normal loading states;
- basic accessibility checks pass.

## Test and Verification Plan

The user's environment will run `pnpm` checks after each implementation pass.
The expected local verification commands are:

```sh
cd web
pnpm run build
```

When tests are added, also run the chosen test command from `web/`.

Recommended test tooling for a later implementation step:

- begin with TypeScript-level pure tests for API and state logic;
- add Playwright only when the server can serve or proxy the client reliably;
- avoid test-only production authentication routes unless a separate plan
  explicitly approves them.

Manual verification before the first web-client PR is considered complete:

1. unauthenticated load shows login;
2. OIDC login reaches the app shell;
3. admission-code login start posts a form, not a URL token;
4. session load returns user and CSRF;
5. create conversation;
6. post message;
7. reload and recover conversation/message state;
8. connect WebSocket with `chat.v1`;
9. receive `message_posted` and fetch the message over HTTP;
10. logout clears UI state and closes realtime.

## Out of Scope for the First Implementation

- member management UI;
- user search;
- profile editing;
- conversation rename/delete;
- message edit/delete;
- reactions;
- typing indicators;
- presence;
- service worker;
- IndexedDB/localStorage message cache;
- automatic mutation retry;
- virtualized message list;
- durable realtime replay;
- cross-origin deployment;
- static asset embedding in `chat-server`.

Static asset serving and single-binary packaging should be planned after the
standalone production client exists and has passed the HTTP/realtime workflow.

## Risks and Mitigations

| Risk | Mitigation |
| --- | --- |
| Frontend state grows implicit | Keep stores small and explicit; avoid global framework patterns |
| Lit component boundaries become too granular | Start with six product components and split only when behavior demands it |
| POST network failure creates duplicates | Do not auto-retry; show unknown result and refresh path |
| WebSocket is mistaken for durable state | Fetch over HTTP after subscribe, reconnect, focus, and notification |
| Screen-reader announcements become noisy | Use bounded polite announcements and prefer visible status text |
| CSS grows into accidental theme debt | Establish tokens and layout files before component styling expands |
| Starter assets leak into product UI | Delete starter component and imports in Step 1 |

## Recommendation

Proceed with Step 1 as the first code change: replace the Vite/Lit starter with
a production app shell and stable file layout. Keep that change intentionally
small. Once the shell is in place and the user's `pnpm run build` passes, move
to API client and session handling.

This gives the project a clean frontend foundation before product behavior is
added, while preserving the server's existing reliability and security
contracts.
