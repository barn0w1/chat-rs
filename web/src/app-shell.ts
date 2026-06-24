import { LitElement, css, html } from 'lit'
import { customElement, state } from 'lit/decorators.js'
import './components/chat-login'
import { SessionStore } from './state/session-store'
import type { SessionSnapshot } from './state/session-store'
import type { Session } from './api/types'

const previewConversations = [
  {
    id: '42',
    title: 'General',
    meta: 'session loaded',
    selected: true,
  },
  {
    id: '41',
    title: 'Operations',
    meta: 'HTTP chat next',
    selected: false,
  },
]

const previewMessages = [
  {
    id: '99',
    time: '16:35',
    author: 'chat-rs',
    body: 'Session loading is connected. Conversation and message reads come next.',
  },
  {
    id: '98',
    time: '16:34',
    author: 'system',
    body: 'HTTP remains the source of truth; realtime will stay a notification channel.',
  },
  {
    id: '97',
    time: '16:33',
    author: 'ui',
    body: 'Message display is moving toward dense IRC-style rows for long reading sessions.',
  },
]

@customElement('chat-app')
export class ChatApp extends LitElement {
  private readonly sessionStore = new SessionStore()
  private unsubscribeSession?: () => void

  @state()
  private sessionSnapshot: SessionSnapshot = this.sessionStore.current

  connectedCallback() {
    super.connectedCallback()
    this.unsubscribeSession = this.sessionStore.subscribe(() => {
      this.sessionSnapshot = this.sessionStore.current
    })
    if (this.sessionSnapshot.status === 'idle') {
      this.sessionStore.load()
    }
  }

  disconnectedCallback() {
    this.unsubscribeSession?.()
    this.sessionStore.dispose()
    super.disconnectedCallback()
  }

  render() {
    const session = this.sessionSnapshot.session
    return html`
      <main class="app-shell" aria-label="chat-rs web client">
        <aside class="sidebar" aria-label="Conversations and session">
          <header class="sidebar-header">
            <div>
              <p class="eyebrow">chat-rs</p>
              <h1>Conversations</h1>
            </div>
          </header>

          <div class="status-line" role="status">
            <span class="status-dot" aria-hidden="true"></span>
            ${this.statusText()}
          </div>

          ${session === undefined ? this.renderSignedOutSidebar() : this.renderSignedInSidebar(session)}
        </aside>

        <section class="content-panel" aria-label="Main content">
          ${this.renderContent()}
        </section>
      </main>
    `
  }

  private renderSignedOutSidebar() {
    return html`
      <div class="sidebar-note">
        Sign in to load your local server session. Private chat content stays out
        of browser storage.
      </div>
    `
  }

  private renderSignedInSidebar(session: Session) {
    return html`
      <section class="session-card" aria-label="Current session">
        <div>
          <p class="label">Signed in as</p>
          <p class="user-name">${session.user.display_name}</p>
        </div>
        <button type="button" @click=${this.logout} ?disabled=${this.sessionSnapshot.status === 'loading'}>
          Sign out
        </button>
      </section>

      <nav class="conversation-list" aria-label="Conversation list">
        ${previewConversations.map(
          (conversation) => html`
            <button
              class=${conversation.selected ? 'conversation selected' : 'conversation'}
              type="button"
              aria-current=${conversation.selected ? 'page' : 'false'}
              data-conversation-id=${conversation.id}
            >
              <span class="conversation-title">${conversation.title}</span>
              <span class="conversation-meta">${conversation.meta}</span>
            </button>
          `,
        )}
      </nav>
    `
  }

  private renderContent() {
    switch (this.sessionSnapshot.status) {
      case 'idle':
      case 'loading':
        return this.renderLoading()
      case 'authenticated':
        return this.renderAuthenticated()
      case 'unauthenticated':
        return html`
          <chat-login
            status-message="No active session."
            @retry-session=${this.loadSession}
          ></chat-login>
        `
      case 'error':
        if (this.sessionSnapshot.session !== undefined) {
          return this.renderAuthenticated()
        }
        return html`
          <chat-login
            .statusMessage=${this.sessionSnapshot.message ?? 'Session check failed.'}
            @retry-session=${this.loadSession}
          ></chat-login>
        `
    }
  }

  private renderLoading() {
    return html`
      <section class="loading-panel" aria-labelledby="loading-title">
        <p class="eyebrow">session</p>
        <h2 id="loading-title">Checking session</h2>
        <p>Reading `/api/v1/session` from the same origin.</p>
      </section>
    `
  }

  private renderAuthenticated() {
    const session = this.sessionSnapshot.session
    if (session === undefined) {
      return this.renderLoading()
    }

    return html`
      <header class="chat-header">
        <div>
          <p class="eyebrow">General</p>
          <h2>Session ready</h2>
        </div>
        <div class="session-pill">${session.user.display_name}</div>
      </header>

      <ol class="message-log" aria-label="Message history preview">
        ${previewMessages.map(
          (message) => html`
            <li class="message-row" data-message-id=${message.id}>
              <time>${message.time}</time>
              <span class="author">${message.author}</span>
              <span class="message-body">${message.body}</span>
            </li>
          `,
        )}
      </ol>

      <form class="composer" aria-label="Message composer preview">
        <textarea
          rows="2"
          placeholder="Conversation and message APIs will be wired in the next step."
          disabled
        ></textarea>
        <button type="submit" disabled>Send</button>
      </form>
    `
  }

  private statusText(): string {
    switch (this.sessionSnapshot.status) {
      case 'idle':
        return 'Not checked'
      case 'loading':
        return 'Checking session'
      case 'authenticated':
        return 'Session ready'
      case 'unauthenticated':
        return 'Signed out'
      case 'error':
        return 'Session check failed'
    }
  }

  private loadSession() {
    this.sessionStore.load()
  }

  private logout() {
    this.sessionStore.logout()
  }

  static styles = css`
    :host {
      display: block;
      min-height: 100svh;
      color: var(--text);
      background: var(--bg);
    }

    .app-shell {
      display: grid;
      grid-template-columns: minmax(260px, 320px) minmax(0, 1fr);
      min-height: 100svh;
      max-width: 1280px;
      margin: 0 auto;
      border-inline: 1px solid var(--border);
      background: var(--surface);
      box-shadow: var(--shadow);
    }

    .sidebar {
      display: flex;
      min-width: 0;
      flex-direction: column;
      gap: var(--space-4);
      padding: var(--space-5);
      border-right: 1px solid var(--border);
      background: var(--surface-muted);
    }

    .sidebar-header,
    .chat-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: var(--space-4);
    }

    h1,
    h2,
    p {
      margin: 0;
    }

    h1 {
      margin-top: var(--space-1);
      font-size: 1.35rem;
      line-height: 1.2;
    }

    h2 {
      margin-top: var(--space-1);
      font-size: 1.12rem;
      line-height: 1.3;
    }

    .eyebrow,
    .conversation-meta,
    .label,
    .session-pill,
    .status-line,
    time {
      color: var(--text-muted);
      font-size: 0.82rem;
      line-height: 1.35;
    }

    .eyebrow,
    .label,
    time,
    .author {
      font-family: var(--font-mono);
    }

    .eyebrow,
    .label {
      letter-spacing: 0.06em;
      text-transform: uppercase;
    }

    .status-line {
      display: inline-flex;
      align-items: center;
      gap: var(--space-2);
    }

    .status-dot {
      width: 8px;
      height: 8px;
      border: 1px solid var(--border-strong);
      border-radius: 999px;
      background: var(--accent);
    }

    .sidebar-note,
    .session-card,
    .loading-panel {
      line-height: 1.55;
    }

    .sidebar-note,
    .session-card,
    .loading-panel,
    .conversation.selected {
      border: 1px solid var(--border);
      background: var(--surface-raised);
    }

    .sidebar-note,
    .session-card,
    .loading-panel {
      padding: var(--space-4);
    }

    .session-card {
      display: grid;
      gap: var(--space-3);
    }

    .user-name {
      margin-top: var(--space-1);
      font-weight: 700;
      overflow-wrap: anywhere;
    }

    button,
    .composer button {
      min-height: 38px;
      border: 1px solid var(--border-strong);
      color: var(--surface-raised);
      background: var(--accent);
      font-weight: 700;
      cursor: pointer;
    }

    button:disabled,
    .composer button:disabled {
      color: var(--text-muted);
      background: var(--surface-sunken);
      border-color: var(--border);
      cursor: not-allowed;
    }

    .conversation-list {
      display: grid;
      gap: var(--space-2);
    }

    .conversation {
      display: grid;
      width: 100%;
      gap: var(--space-1);
      padding: var(--space-3);
      border: 1px solid transparent;
      border-radius: var(--radius-md);
      color: var(--text);
      background: transparent;
      text-align: left;
    }

    .conversation-title {
      overflow: hidden;
      font-weight: 700;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .content-panel {
      display: grid;
      min-width: 0;
      grid-template-rows: auto minmax(0, 1fr) auto;
      background: var(--surface);
    }

    chat-login,
    .loading-panel {
      align-self: center;
      justify-self: center;
      width: min(100% - 32px, 680px);
    }

    .chat-header {
      min-width: 0;
      padding: var(--space-5) var(--space-6);
      border-bottom: 1px solid var(--border);
    }

    .session-pill {
      flex: 0 0 auto;
      padding: 5px 9px;
      border: 1px solid var(--border);
      border-radius: 999px;
      background: var(--surface-muted);
    }

    .message-log {
      display: grid;
      align-content: start;
      min-width: 0;
      margin: 0;
      padding: var(--space-4) var(--space-6);
      overflow: auto;
      list-style: none;
    }

    .message-row {
      display: grid;
      grid-template-columns: 5.5ch 14ch minmax(0, 1fr);
      gap: var(--space-3);
      min-width: 0;
      padding: 6px 0;
      border-bottom: 1px solid var(--border);
      line-height: 1.55;
    }

    .message-row:last-child {
      border-bottom: 0;
    }

    .author {
      color: var(--accent-strong);
      font-weight: 700;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .message-body {
      min-width: 0;
      overflow-wrap: anywhere;
    }

    .composer {
      display: grid;
      grid-template-columns: minmax(0, 1fr) auto;
      gap: var(--space-3);
      padding: var(--space-4) var(--space-6) var(--space-5);
      border-top: 1px solid var(--border);
      background: var(--surface-raised);
    }

    .composer textarea {
      width: 100%;
      min-height: 48px;
      resize: vertical;
      border: 1px solid var(--border);
      border-radius: var(--radius-md);
      padding: var(--space-3);
      color: var(--text);
      background: var(--surface);
      line-height: 1.5;
    }

    .composer button {
      min-width: 84px;
      padding-inline: var(--space-4);
    }

    @media (max-width: 760px) {
      .app-shell {
        grid-template-columns: 1fr;
        border-inline: 0;
        box-shadow: none;
      }

      .sidebar {
        border-right: 0;
        border-bottom: 1px solid var(--border);
      }

      .conversation-list {
        display: flex;
        gap: var(--space-2);
        overflow-x: auto;
        padding-bottom: var(--space-1);
      }

      .conversation {
        min-width: 190px;
      }

      .chat-header,
      .message-log,
      .composer {
        padding-inline: var(--space-4);
      }

      .chat-header {
        align-items: flex-start;
        flex-direction: column;
      }

      .message-row {
        grid-template-columns: 5.5ch minmax(0, 1fr);
      }

      .author {
        grid-column: 2;
      }

      .message-body {
        grid-column: 2;
      }

      .composer {
        grid-template-columns: 1fr;
      }

      .composer button {
        min-height: 44px;
      }
    }
  `
}
