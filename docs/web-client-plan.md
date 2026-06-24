# Web Client Direction and Plan

Status: planning only; no implementation has started  
Date: 2026-06-24  
Target phase: Milestone 6, browser client and single-binary packaging

## Purpose

The next phase should build the production browser client for `chat-rs`.
The desired direction is not "feature-rich first." It is a small, fast,
carefully finished, web-standard-oriented client that fits the Rust server's
current quality bar:

- simple operational shape;
- durable state from SQLite-backed HTTP;
- realtime as a recoverable hint channel;
- strict security boundaries;
- low dependency weight;
- long-lived browser primitives;
- a UI that feels complete because its basic interactions are deeply polished.

The product analogy is a simple system with high craft: few ideas, well joined.
For `chat-rs`, that means conversation list, message history, message composer,
session/admission flow, reconnect, loading/error states, keyboard behavior,
and accessibility must feel intentional before adding broader social features.

## Recommended Direction

Use a Baseline-first web client built with:

| Area | Recommendation |
| --- | --- |
| Language | TypeScript |
| Build tool | Vite |
| Component model | Web Components via Lit |
| Styling | CSS modules/files with custom properties and cascade layers |
| Routing | minimal same-document client state; no router until needed |
| State | small explicit stores/controllers, not a global framework |
| Network | typed Fetch wrapper plus one WebSocket coordinator |
| Offline/retry | conservative refresh/reconnect only; no automatic POST retry yet |
| Packaging | hashed static assets embedded/served by `chat-server` |

This is intentionally not a conventional React/Vue/Svelte SPA recommendation.
Those frameworks can all build a good chat client, but `chat-rs` benefits from
a thinner layer because the server API is already strongly shaped and the UI
surface is initially modest.

## Why This Fits `chat-rs`

### Web Standards First

Web Components are browser-native component primitives: custom elements, shadow
DOM, templates, slots, and standard DOM events. Lit sits close to those
primitives and mainly supplies efficient declarative rendering, reactive
properties, and ergonomic templates.

That matches the existing Rust architecture:

- `chat` is not coupled to infrastructure;
- `chat-server` adapts protocols explicitly;
- the web client can likewise keep product state independent of a heavy UI
  framework.

### Small Runtime and Long Life

The client should remain understandable in five years. A Lit/Web Components
approach makes the durable parts native:

- elements are standard custom elements;
- events are DOM events;
- styling uses CSS;
- server communication uses Fetch and WebSocket;
- build output is ordinary static assets.

If Lit were ever removed, much of the app can still remain as custom elements
and plain TypeScript. That is a better long-term risk profile than tying the
entire UI model to a large framework-specific runtime.

### Performance by Default

The first production UI should be fast because it does less:

- ship little JavaScript;
- avoid a large component framework;
- render bounded pages;
- append/merge messages by ID;
- avoid re-rendering the whole app for one notification;
- use HTTP snapshots as truth and WebSocket messages as invalidation hints;
- measure Core Web Vitals and interaction latency from the beginning.

The server already uses explicit page sizes and bounded realtime queues. The
client should preserve that same philosophy.

## Evaluated Options

### Option A: Plain TypeScript and Native Web Components

Pros:

- closest to platform primitives;
- smallest possible dependency surface;
- no framework lock-in;
- easy static asset packaging.

Cons:

- hand-written rendering and update logic can become uneven;
- more boilerplate around property reflection, templating, and lifecycle;
- higher risk of accidental DOM bugs in a project whose author is not primarily
  a frontend specialist.

Assessment: philosophically excellent, but slightly too spartan for a polished
first client. It risks spending craft on plumbing rather than product quality.

### Option B: TypeScript, Lit, and Web Components

Pros:

- still web-standard-oriented;
- small and mature;
- excellent fit for custom-element boundaries;
- declarative rendering without a large framework;
- good ergonomics for a compact app;
- easy to compose with plain CSS, Fetch, WebSocket, and native forms.

Cons:

- still a dependency;
- SSR/hydration is not the initial sweet spot;
- team must learn Lit conventions.

Assessment: recommended. This is the best balance between web-native design,
speed, maintainability, and implementation quality.

### Option C: Svelte

Pros:

- highly productive;
- compiler-oriented;
- small output for many apps;
- approachable syntax.

Cons:

- component model is framework-specific rather than native custom elements by
  default;
- routing/app architecture choices tend to pull in framework conventions;
- less aligned with the project's "small stable boundary" instinct.

Assessment: strong alternative if Lit feels too low-level after a spike, but
not the first choice for a web-standard-shaped client.

### Option D: Preact

Pros:

- small;
- React-like mental model;
- mature ecosystem;
- easy to hire/borrow knowledge for.

Cons:

- still virtual-DOM-framework shaped;
- less platform-native than Web Components;
- app may inherit React-style global patterns that are unnecessary here.

Assessment: good fallback if the priority becomes mainstream familiarity over
web-native shape.

### Option E: React

Pros:

- largest ecosystem;
- many UI and testing resources;
- familiar to many frontend developers.

Cons:

- heavier conceptual and dependency surface than the current product needs;
- encourages SPA architecture choices before the app requires them;
- less aligned with the "small, fast, durable primitive" goal.

Assessment: not recommended for the first production client.

### Option F: Solid

Pros:

- fine-grained reactivity;
- strong performance orientation;
- small runtime.

Cons:

- framework-specific model;
- smaller ecosystem;
- less web-standard-oriented than Web Components.

Assessment: technically attractive, but not the best match for this project's
long-lived standards-first direction.

## Product Shape for the First Client

The first production client should be complete, not broad.

### Must Have

- session detection on load;
- login existing user;
- login with admission code;
- logout;
- conversation list;
- create conversation;
- open conversation;
- paginated message history;
- post message;
- WebSocket connect with `chat.v1`;
- subscribe after `ready`;
- snapshot after `subscribed`;
- merge snapshots and notifications by message ID;
- reconnect with backoff;
- explicit offline/disconnected state;
- error states using stable server problem types;
- responsive layout for mobile and desktop;
- keyboard-accessible message composer;
- no automatic ambiguous POST retry.

### Should Have

- unread or "new messages" hint within the current session;
- scroll anchoring when older messages load;
- focus management after sending;
- clear pending/sending state;
- reduced-motion support;
- no-storage-by-default posture for secrets;
- theme tokens ready for future customization.

### Must Not Have Yet

- member add/remove UI;
- user search;
- profile editing;
- message edit/delete;
- reactions;
- typing indicators;
- presence;
- offline message queue;
- automatic mutation retry;
- durable local cache of private content;
- service worker.

Those are not bad features. They are simply not supported by the server's
current product contract or reliability model.

## Client Architecture

### Proposed Directory Layout

```text
web/
|-- package.json
|-- index.html
|-- vite.config.ts
|-- src/
|   |-- main.ts
|   |-- app-shell.ts
|   |-- api/
|   |   |-- client.ts
|   |   |-- types.ts
|   |   `-- problems.ts
|   |-- realtime/
|   |   |-- socket.ts
|   |   `-- protocol.ts
|   |-- state/
|   |   |-- session-store.ts
|   |   |-- conversation-store.ts
|   |   `-- message-store.ts
|   |-- components/
|   |   |-- chat-login.ts
|   |   |-- chat-layout.ts
|   |   |-- conversation-list.ts
|   |   |-- message-list.ts
|   |   |-- message-composer.ts
|   |   `-- connection-status.ts
|   `-- styles/
|       |-- base.css
|       |-- tokens.css
|       `-- layout.css
`-- e2e/
    `-- realtime.html
```

`web/e2e/realtime.html` should remain a temporary harness until the production
client covers the same behavior. Later it can be removed or moved out of the
production asset path.

### State Model

Use small explicit state containers. Avoid a global framework store initially.

Suggested state:

- `SessionStore`
  - authenticated user;
  - CSRF token;
  - session loading/error state.
- `ConversationStore`
  - conversation page cache for the active session;
  - selected conversation ID;
  - pagination cursor.
- `MessageStore`
  - messages by conversation ID;
  - message IDs sorted newest/oldest as needed by view;
  - page cursors;
  - pending local send states.
- `RealtimeCoordinator`
  - socket lifecycle;
  - subscription lifecycle;
  - reconnect backoff;
  - notification dispatch.

Keep the source of truth clear:

- HTTP mutation response is authoritative for the caller.
- HTTP reads are authoritative for state reconstruction.
- WebSocket messages only trigger targeted HTTP fetches or local invalidation.

### Realtime Algorithm

For a selected conversation:

1. ensure session exists;
2. open `new WebSocket("/api/v1/ws", "chat.v1")`;
3. wait for `ready`;
4. send `subscribe`;
5. wait for `subscribed`;
6. fetch latest messages over HTTP;
7. merge messages by `message.id`;
8. on `message_posted`, fetch that one message by ID;
9. on reconnect, repeat subscribe then fetch latest page;
10. never assume missed notifications are replayed.

For `conversation_created`:

1. merge the conversation from the POST response if this tab created it;
2. otherwise refresh the first conversation page or fetch the specific resource
   when such endpoint semantics are sufficient;
3. deduplicate by conversation ID.

### Mutation Policy

Until idempotency exists, POST requests must be treated as non-retryable after
the request leaves the browser.

Safe behavior:

- disable the send/create control while one request is in flight;
- if the network fails after dispatch, show an "unknown result" state;
- prompt the user to refresh before retrying;
- do not silently resend the same POST.

This is stricter than many chat clients, but it is correct for the current
server contract.

### Persistence in the Browser

Initial client should keep sensitive and private state in memory:

- CSRF token in memory only;
- no localStorage for session, CSRF, admission codes, or message content;
- no IndexedDB in the first milestone;
- browser cookies remain HttpOnly and server-owned.

The first durable local storage feature should be deliberately designed, not
added as a convenience cache.

## UI Direction

### Design Principles

- The first screen is the usable chat app, not a marketing page.
- Empty states should be small and functional.
- Controls should be familiar: buttons, inputs, textareas, lists, dialogs.
- Visual style should be restrained, crisp, and fast.
- The UI should expose system state honestly: connecting, disconnected,
  refreshing, sending, unknown mutation result.
- Avoid decorative complexity until the interaction model is excellent.

### Layout

Desktop:

- left column: conversation list;
- main column: selected conversation messages and composer;
- top/right compact session and connection controls.

Mobile:

- conversation list and conversation detail as two navigable panes;
- preserve browser back behavior if client-side view switching is used;
- composer fixed within the conversation view without covering messages.

### Message History

Important details:

- preserve scroll position when loading older pages;
- keep the newest messages reachable without jarring jumps;
- group only if grouping does not complicate accessibility;
- show timestamps from server milliseconds;
- dedupe by message ID;
- do not trust realtime order as durable order.

### Accessibility

Minimum target:

- semantic buttons and forms;
- visible focus states;
- keyboard navigation through conversation list and composer;
- `aria-live` or equivalent carefully scoped for new message announcements;
- reduced motion support;
- color contrast checked;
- no keyboard trap around the composer;
- errors associated with relevant controls.

Chat UIs can easily become noisy for screen-reader users. New-message
announcements should be polite, bounded, and user-respectful.

## Security Requirements

The client must preserve the server's current security posture:

- use same-origin Fetch;
- include credentials only through normal same-origin browser behavior;
- never read or store the HttpOnly session cookie;
- keep CSRF token in memory;
- send `X-CSRF-Token` only on unsafe API requests;
- never put CSRF, admission code, or session material into URLs;
- never log secrets to console;
- avoid third-party scripts in the production client;
- serve production assets with no-store or hashed immutable caching according
  to server packaging design;
- preserve exact `chat.v1` WebSocket subprotocol;
- do not invent a WebSocket bearer token.

## Performance Targets

Initial targets should be modest and measurable:

| Metric | Target |
| --- | ---: |
| initial JS gzip | under 50 KiB if practical |
| first route usable on local production build | under 1 second on a mid-range laptop |
| message send UI acknowledgement | immediate pending state |
| conversation switch | no full app reload |
| long message list | bounded DOM through pagination before virtualization |

Do not add virtualization until real message counts prove it is needed.
Pagination and careful DOM updates should be enough for the first production
client.

Use measurement early:

- Vite production bundle report or equivalent manual size check;
- Lighthouse or WebPageTest-style Core Web Vitals smoke checks;
- browser performance recording for message append and conversation switch;
- Playwright screenshots for desktop and mobile layouts.

## Build and Packaging Plan

### Development Build

Use Vite for:

- fast local development;
- TypeScript build integration;
- production asset hashing;
- simple static output.

### Server Integration

Milestone 6 should add a `chat-server` static asset layer after the web build
exists.

Expected behavior:

- API and auth routes continue to win over static serving.
- `/` serves the web client.
- hashed assets are served from a stable asset path.
- unknown client-side app paths may serve `index.html` only if the client adds
  real same-document routing.
- E2E harness is not exposed in production by default.

Avoid coupling the browser client to the loopback listen address or reverse
proxy details. The client should use same-origin relative URLs:

- `/api/v1/session`
- `/api/v1/conversations`
- `/api/v1/ws`
- `/auth/oidc/start`

## Implementation Phases

### Phase 0: Frontend Skeleton Spike

Goal: validate the selected stack without building product behavior.

Tasks:

- add Vite + TypeScript + Lit;
- create one custom element app shell;
- produce a production build;
- measure bundle size;
- confirm generated assets can be served statically;
- decide whether Lit ergonomics are acceptable.

Exit criteria:

- build is small and understandable;
- no server API behavior is changed;
- stack choice is either confirmed or replaced before product work.

### Phase 1: Session and Login Surface

Tasks:

- session fetch;
- authenticated/unauthenticated app shell;
- login existing;
- admission-code login form;
- logout;
- finite problem handling;
- no secret storage.

Exit criteria:

- a user can log in and out through the production client;
- CSRF token is held only in memory;
- exact server error states are visible enough for users.

### Phase 2: HTTP Chat MVP

Tasks:

- list conversations;
- create conversation;
- open conversation;
- list messages;
- post message;
- fetch one message by ID;
- pagination for conversations and messages;
- loading/empty/error states.

Exit criteria:

- the client can perform the E2E HTTP workflow without dev-console scripts;
- no automatic POST retry exists;
- all IDs are handled as strings in the client.

### Phase 3: Realtime Integration

Tasks:

- WebSocket coordinator;
- `chat.v1` connection;
- ready/subscribe flow;
- reconnect backoff;
- message notification handling;
- conversation-created notification handling;
- logout/session-ended handling;
- visible connection state.

Exit criteria:

- the production client covers the current realtime E2E harness happy path;
- reconnect rebuilds state over HTTP;
- duplicate notifications are harmless.

### Phase 4: Polish and Accessibility Gate

Tasks:

- responsive layout;
- keyboard navigation;
- focus management;
- scroll anchoring;
- reduced motion;
- screen-reader announcement policy;
- visual design pass;
- empty/error/loading polish.

Exit criteria:

- app is usable on desktop and mobile;
- basic accessibility checks pass;
- the UI feels complete for the limited feature set.

### Phase 5: Single-Binary Packaging

Tasks:

- embed or include production assets in `chat-server`;
- route `/` and assets;
- set cache headers;
- ensure API/auth/ws routes remain unaffected;
- remove production exposure of temporary E2E harness;
- add packaging docs.

Exit criteria:

- one binary can run the server and serve the web client;
- production-like E2E no longer needs temporary Caddy HTML helpers.

## Test Strategy

### Unit Tests

Use focused tests for pure client logic:

- API problem parsing;
- ID-string handling;
- message merge/dedupe;
- pagination cursor state;
- realtime protocol message parsing;
- reconnect backoff calculation.

### Component Tests

Use browser-based tests only where useful:

- login form validation;
- composer behavior;
- conversation selection;
- message list rendering;
- error states.

### End-to-End Tests

Use Playwright or equivalent browser automation after the server can serve the
client locally.

Initial E2E scenarios:

- unauthenticated load shows login choices;
- session load shows chat shell;
- create conversation;
- post message;
- reload and recover history;
- connect realtime and receive notification;
- logout closes session and socket;
- mobile viewport smoke screenshot.

Avoid test-only authentication bypass unless a later plan explicitly designs
one. The current project has deliberately avoided test-only production routes.

## Open Decisions Before Implementation

1. Confirm Lit after a small spike.
2. Decide whether `web/e2e/realtime.html` remains in repo after production
   client covers those checks.
3. Decide static asset embedding mechanism in Rust.
4. Decide cache headers for `index.html` versus hashed assets.
5. Decide whether the first client uses client-side route paths or only
   internal view state.
6. Decide visual language and theme tokens before CSS grows.
7. Decide accessibility announcement policy for new messages.

## Sources Consulted

These are official or primary references used to shape this plan:

- [MDN: Web Components](https://developer.mozilla.org/en-US/docs/Web/API/Web_components)
- [MDN: Using custom elements](https://developer.mozilla.org/en-US/docs/Web/API/Web_components/Using_custom_elements)
- [MDN: WebSocket API](https://developer.mozilla.org/en-US/docs/Web/API/WebSockets_API)
- [MDN: Fetch API](https://developer.mozilla.org/en-US/docs/Web/API/Fetch_API)
- [MDN: AbortController](https://developer.mozilla.org/en-US/docs/Web/API/AbortController)
- [MDN: Baseline](https://developer.mozilla.org/en-US/docs/Glossary/Baseline/Compatibility)
- [web.dev: Core Web Vitals](https://web.dev/articles/vitals)
- [web.dev: Interaction to Next Paint](https://web.dev/articles/inp)
- [Lit documentation](https://lit.dev/docs/)
- [Vite guide](https://vite.dev/guide/)
- [TypeScript handbook](https://www.typescriptlang.org/docs/handbook/intro.html)
- [WAI-ARIA Authoring Practices Guide](https://www.w3.org/WAI/ARIA/apg/)
- [React documentation](https://react.dev/)
- [Preact documentation](https://preactjs.com/)
- [Svelte documentation](https://svelte.dev/docs)
- [Solid documentation](https://docs.solidjs.com/)

## Recommendation

Proceed with a short Phase 0 stack spike using TypeScript, Vite, and Lit. If
the spike confirms a small bundle and clean code shape, continue into the
production client phases above.

This path is closest to the project's existing taste: explicit boundaries,
small abstractions, durable primitives, and careful behavior over broad
feature count.
