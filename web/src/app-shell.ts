import { LitElement, css, html } from 'lit'
import { customElement, state } from 'lit/decorators.js'
import './components/connection-status'
import './components/chat-login'
import type { Conversation, Message, Session } from './api/types'
import { RealtimeSocket } from './realtime/socket'
import type { RealtimeServerEvent } from './realtime/protocol'
import type { RealtimeSnapshot } from './realtime/socket'
import { ConversationStore } from './state/conversation-store'
import type { ConversationSnapshot } from './state/conversation-store'
import { MessageStore } from './state/message-store'
import type { MessageSnapshot } from './state/message-store'
import { SessionStore } from './state/session-store'
import type { SessionSnapshot } from './state/session-store'

const timeFormatter = new Intl.DateTimeFormat(undefined, {
  hour: '2-digit',
  minute: '2-digit',
})

@customElement('chat-app')
export class ChatApp extends LitElement {
  private readonly sessionStore = new SessionStore()
  private readonly conversationStore = new ConversationStore()
  private readonly messageStore = new MessageStore()
  private readonly realtimeSocket = new RealtimeSocket()

  private unsubscribeSession?: () => void
  private unsubscribeConversations?: () => void
  private unsubscribeMessages?: () => void
  private unsubscribeRealtime?: () => void
  private unsubscribeRealtimeEvents?: () => void

  @state()
  private sessionSnapshot: SessionSnapshot = this.sessionStore.current

  @state()
  private conversationSnapshot: ConversationSnapshot = this.conversationStore.current

  @state()
  private messageSnapshot: MessageSnapshot = this.messageStore.current

  @state()
  private realtimeSnapshot: RealtimeSnapshot = this.realtimeSocket.current

  connectedCallback() {
    super.connectedCallback()
    this.unsubscribeSession = this.sessionStore.subscribe(() => this.onSessionChange())
    this.unsubscribeConversations = this.conversationStore.subscribe(() =>
      this.onConversationChange(),
    )
    this.unsubscribeMessages = this.messageStore.subscribe(() => {
      this.messageSnapshot = this.messageStore.current
    })
    this.unsubscribeRealtime = this.realtimeSocket.subscribe(() => {
      this.realtimeSnapshot = this.realtimeSocket.current
    })
    this.unsubscribeRealtimeEvents = this.realtimeSocket.subscribeEvents((event) =>
      this.handleRealtimeEvent(event),
    )

    if (this.sessionSnapshot.status === 'idle') {
      this.sessionStore.load()
    }
  }

  disconnectedCallback() {
    this.unsubscribeSession?.()
    this.unsubscribeConversations?.()
    this.unsubscribeMessages?.()
    this.unsubscribeRealtime?.()
    this.unsubscribeRealtimeEvents?.()
    this.sessionStore.dispose()
    this.conversationStore.dispose()
    this.messageStore.dispose()
    this.realtimeSocket.stop()
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
            ${this.renderRefreshButton()}
          </header>

          <div class="status-line" role="status">
            <span class="status-dot" aria-hidden="true"></span>
            ${this.statusText()}
          </div>
          <connection-status
            .status=${this.realtimeSnapshot.status}
            .message=${this.realtimeStatusMessage()}
          ></connection-status>

          ${session === undefined ? this.renderSignedOutSidebar() : this.renderSignedInSidebar(session)}
        </aside>

        <section class="content-panel" aria-label="Main content">
          ${this.renderContent()}
        </section>
      </main>
    `
  }

  private onSessionChange(): void {
    const previousSession = this.sessionSnapshot.session
    this.sessionSnapshot = this.sessionStore.current
    const nextSession = this.sessionSnapshot.session

    if (previousSession === undefined && nextSession !== undefined) {
      this.conversationStore.load()
      this.realtimeSocket.start()
      return
    }

    if (previousSession !== undefined && nextSession === undefined) {
      this.conversationStore.clear()
      this.messageStore.clear()
      this.realtimeSocket.stop()
    }
  }

  private onConversationChange(): void {
    const previousSelectedId = this.conversationSnapshot.selectedId
    this.conversationSnapshot = this.conversationStore.current
    const selectedId = this.conversationSnapshot.selectedId

    if (selectedId === undefined) {
      this.messageStore.clear()
      this.realtimeSocket.setSubscription(undefined)
      return
    }

    this.realtimeSocket.setSubscription(selectedId)

    if (
      selectedId !== previousSelectedId ||
      this.messageSnapshot.conversationId !== selectedId
    ) {
      this.messageStore.load(selectedId)
    }
  }

  private renderRefreshButton() {
    const disabled =
      this.sessionSnapshot.status === 'loading' ||
      this.conversationSnapshot.status === 'loading' ||
      this.messageSnapshot.status === 'loading'

    return html`
      <button
        class="refresh-button"
        type="button"
        aria-label="Refresh"
        title="Refresh"
        ?disabled=${disabled}
        @click=${() => this.refreshCurrentView()}
      >
        Refresh
      </button>
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
    const conversations = this.conversationSnapshot.conversations
    return html`
      <section class="session-card" aria-label="Current session">
        <div>
          <p class="label">Signed in as</p>
          <p class="user-name">${session.user.display_name}</p>
        </div>
        <button
          type="button"
          @click=${() => this.logout()}
          ?disabled=${this.sessionSnapshot.status === 'loading'}
        >
          Sign out
        </button>
      </section>

      <form
        class="create-form"
        aria-label="Create conversation"
        @submit=${(event: SubmitEvent) => this.createConversation(event)}
      >
        <label for="conversation-title">New conversation</label>
        <div class="create-row">
          <input
            id="conversation-title"
            name="title"
            type="text"
            autocomplete="off"
            placeholder="Topic"
            maxlength="120"
            ?disabled=${this.conversationSnapshot.status === 'creating'}
          />
          <button
            type="submit"
            ?disabled=${this.conversationSnapshot.status === 'creating'}
          >
            Add
          </button>
        </div>
      </form>

      ${this.renderConversationProblem()}

      <nav class="conversation-list" aria-label="Conversation list">
        ${this.renderConversationList(conversations)}
      </nav>
    `
  }

  private renderConversationList(conversations: Conversation[]) {
    if (this.conversationSnapshot.status === 'loading' && conversations.length === 0) {
      return html`<p class="empty-note">Loading conversations...</p>`
    }

    if (conversations.length === 0) {
      return html`<p class="empty-note">No conversations yet. Create one to begin.</p>`
    }

    return conversations.map((conversation) => {
      const selected = conversation.id === this.conversationSnapshot.selectedId
      return html`
        <button
          class=${selected ? 'conversation selected' : 'conversation'}
          type="button"
          aria-current=${selected ? 'page' : 'false'}
          data-conversation-id=${conversation.id}
          @click=${(event: Event) => this.selectConversation(event)}
        >
          <span class="conversation-title">${conversation.title}</span>
          <span class="conversation-meta">${conversation.role} / ${this.formatDate(conversation.created_at_ms)}</span>
        </button>
      `
    })
  }

  private renderConversationProblem() {
    if (this.conversationSnapshot.message === undefined) {
      return null
    }

    return html`
      <p class="inline-problem" role="status">${this.conversationSnapshot.message}</p>
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
            @retry-session=${() => this.loadSession()}
          ></chat-login>
        `
      case 'error':
        if (this.sessionSnapshot.session !== undefined) {
          return this.renderAuthenticated()
        }
        return html`
          <chat-login
            .statusMessage=${this.sessionSnapshot.message ?? 'Session check failed.'}
            @retry-session=${() => this.loadSession()}
          ></chat-login>
        `
    }
  }

  private renderLoading() {
    return html`
      <section class="loading-panel" aria-labelledby="loading-title">
        <p class="eyebrow">session</p>
        <h2 id="loading-title">Checking session</h2>
        <p>Reading <code>/api/v1/session</code> from the same origin.</p>
      </section>
    `
  }

  private renderAuthenticated() {
    const session = this.sessionSnapshot.session
    if (session === undefined) {
      return this.renderLoading()
    }

    const selectedConversation = this.selectedConversation()
    if (selectedConversation === undefined) {
      return this.renderNoConversation()
    }

    return html`
      <header class="chat-header">
        <div>
          <p class="eyebrow">conversation</p>
          <h2>${selectedConversation.title}</h2>
        </div>
        <div class="session-pill">${session.user.display_name}</div>
      </header>

      <section class="chat-scroll" aria-label="Message history">
        ${this.renderMessageProblem()}
        ${this.renderOlderMessagesButton()}
        ${this.renderMessageLog(session)}
      </section>

      <form
        class="composer"
        aria-label="Message composer"
        @submit=${(event: SubmitEvent) => this.postMessage(event)}
      >
        <label class="visually-hidden" for="message-body">Message</label>
        <textarea
          id="message-body"
          name="body"
          rows="2"
          placeholder="Write a message"
          ?disabled=${this.messageSnapshot.status === 'posting'}
        ></textarea>
        <button
          type="submit"
          ?disabled=${this.messageSnapshot.status === 'posting'}
        >
          Send
        </button>
      </form>
    `
  }

  private renderNoConversation() {
    return html`
      <section class="empty-panel" aria-labelledby="empty-title">
        <p class="eyebrow">conversation</p>
        <h2 id="empty-title">No conversation selected</h2>
        <p>Create a conversation from the sidebar to begin using the HTTP chat API.</p>
      </section>
    `
  }

  private renderMessageProblem() {
    if (this.messageSnapshot.message === undefined) {
      return null
    }

    return html`
      <p class="inline-problem" role="status">${this.messageSnapshot.message}</p>
    `
  }

  private renderOlderMessagesButton() {
    if (this.messageSnapshot.nextCursor === null) {
      return null
    }

    return html`
      <button
        class="older-button"
        type="button"
        ?disabled=${this.messageSnapshot.status === 'loading_older'}
        @click=${() => this.messageStore.loadOlder()}
      >
        ${this.messageSnapshot.status === 'loading_older' ? 'Loading older messages...' : 'Load older messages'}
      </button>
    `
  }

  private renderMessageLog(session: Session) {
    const messages = this.messageSnapshot.messages
    if (this.messageSnapshot.status === 'loading') {
      return html`<p class="empty-note">Loading messages...</p>`
    }

    if (messages.length === 0) {
      return html`<p class="empty-note">No messages yet.</p>`
    }

    return html`
      <ol class="message-log" aria-label="Messages">
        ${messages.map((message) => this.renderMessageRow(message, session))}
      </ol>
    `
  }

  private renderMessageRow(message: Message, session: Session) {
    return html`
      <li class="message-row" data-message-id=${message.id}>
        <time datetime=${new Date(message.created_at_ms).toISOString()}>
          ${timeFormatter.format(new Date(message.created_at_ms))}
        </time>
        <span class="author">${this.authorLabel(message, session)}</span>
        <span class="message-body">${message.body}</span>
      </li>
    `
  }

  private selectedConversation(): Conversation | undefined {
    return this.conversationSnapshot.conversations.find(
      (conversation) => conversation.id === this.conversationSnapshot.selectedId,
    )
  }

  private statusText(): string {
    if (this.sessionSnapshot.status === 'authenticated') {
      switch (this.conversationSnapshot.status) {
        case 'idle':
          return 'Session ready'
        case 'loading':
          return 'Loading conversations'
        case 'ready':
          return 'HTTP chat ready'
        case 'creating':
          return 'Creating conversation'
        case 'error':
          return 'Conversation request failed'
      }
    }

    switch (this.sessionSnapshot.status) {
      case 'idle':
        return 'Not checked'
      case 'loading':
        return 'Checking session'
      case 'unauthenticated':
        return 'Signed out'
      case 'error':
        return 'Session check failed'
    }
  }

  private realtimeStatusMessage(): string {
    if (this.realtimeSnapshot.message !== undefined) {
      return this.realtimeSnapshot.message
    }
    if (this.realtimeSnapshot.subscribedId !== undefined) {
      return `Realtime subscribed to ${this.realtimeSnapshot.subscribedId}`
    }
    return ''
  }

  private handleRealtimeEvent(event: RealtimeServerEvent): void {
    switch (event.type) {
      case 'ready':
        break
      case 'subscribed':
        if (
          event.conversation_id === this.conversationSnapshot.selectedId &&
          this.messageSnapshot.status !== 'posting'
        ) {
          this.messageStore.load(event.conversation_id)
        }
        break
      case 'unsubscribed':
        break
      case 'subscription_rejected':
        break
      case 'conversation_created':
        if (this.conversationSnapshot.status !== 'creating') {
          this.conversationStore.load()
        }
        break
      case 'message_posted':
        if (event.conversation_id === this.conversationSnapshot.selectedId) {
          this.messageStore.fetchOne(event.conversation_id, event.message_id)
        }
        break
    }
  }

  private refreshCurrentView(): void {
    if (this.sessionSnapshot.session === undefined) {
      this.sessionStore.load()
      return
    }

    this.conversationStore.load()
    const selectedId = this.conversationSnapshot.selectedId
    if (selectedId !== undefined) {
      this.messageStore.load(selectedId)
    }
  }

  private loadSession(): void {
    this.sessionStore.load()
  }

  private logout(): void {
    this.sessionStore.logout()
  }

  private createConversation(event: SubmitEvent): void {
    event.preventDefault()
    const form = event.currentTarget as HTMLFormElement
    const title = formDataString(form, 'title')
    const csrfToken = this.sessionSnapshot.session?.csrf_token
    if (csrfToken === undefined) {
      return
    }

    this.conversationStore.create(title, csrfToken)
    form.reset()
  }

  private selectConversation(event: Event): void {
    const button = event.currentTarget as HTMLButtonElement
    const conversationId = button.dataset.conversationId
    if (conversationId !== undefined) {
      this.conversationStore.select(conversationId)
    }
  }

  private postMessage(event: SubmitEvent): void {
    event.preventDefault()
    const form = event.currentTarget as HTMLFormElement
    const body = formDataString(form, 'body')
    const conversationId = this.conversationSnapshot.selectedId
    const csrfToken = this.sessionSnapshot.session?.csrf_token
    if (conversationId === undefined || csrfToken === undefined) {
      return
    }

    this.messageStore.post(conversationId, body, csrfToken)
    form.reset()
  }

  private authorLabel(message: Message, session: Session): string {
    if (message.author_id === session.user.id) {
      return session.user.display_name
    }
    return `user:${message.author_id}`
  }

  private formatDate(createdAtMs: number): string {
    return new Date(createdAtMs).toLocaleDateString(undefined, {
      month: 'short',
      day: 'numeric',
    })
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
      width: 100%;
      min-height: 100svh;
      grid-template-columns: minmax(260px, 328px) minmax(0, 1fr);
      background: var(--surface);
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
    .loading-panel,
    .empty-panel {
      line-height: 1.55;
    }

    .sidebar-note,
    .session-card,
    .loading-panel,
    .empty-panel,
    .conversation.selected {
      border: 1px solid var(--border);
      background: var(--surface-raised);
    }

    .sidebar-note,
    .session-card,
    .loading-panel,
    .empty-panel {
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
      background: var(--surface-muted);
      border-color: var(--border);
      cursor: not-allowed;
    }

    .refresh-button {
      display: inline-grid;
      min-height: 36px;
      place-items: center;
      padding: 0 var(--space-3);
      color: var(--text);
      background: var(--surface-raised);
      font-size: 0.82rem;
    }

    .create-form {
      display: grid;
      gap: var(--space-2);
    }

    .create-row {
      display: grid;
      grid-template-columns: minmax(0, 1fr) auto;
      gap: var(--space-2);
    }

    label {
      color: var(--text-muted);
      font-family: var(--font-mono);
      font-size: 0.86rem;
    }

    input,
    textarea {
      border: 1px solid var(--border);
      border-radius: var(--radius-md);
      color: var(--text);
      background: var(--surface);
      font: inherit;
    }

    input {
      min-width: 0;
      min-height: 38px;
      padding: 0 var(--space-3);
    }

    .create-row button {
      padding-inline: var(--space-3);
    }

    .conversation-list {
      display: grid;
      align-content: start;
      gap: var(--space-2);
      overflow: auto;
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

    .conversation:hover {
      border-color: var(--border);
      background: var(--surface-raised);
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
      min-height: 100svh;
      grid-template-rows: auto minmax(0, 1fr) auto;
      background: var(--surface);
    }

    chat-login,
    .loading-panel,
    .empty-panel {
      align-self: center;
      justify-self: center;
      width: min(100% - 32px, 680px);
    }

    .chat-header {
      min-width: 0;
      padding: var(--space-5) var(--space-6);
      border-bottom: 1px solid var(--border);
    }

    .chat-header h2 {
      overflow: hidden;
      max-width: 72ch;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .session-pill {
      flex: 0 0 auto;
      padding: 5px 9px;
      border: 1px solid var(--border);
      border-radius: 999px;
      background: var(--surface-muted);
    }

    .chat-scroll {
      display: grid;
      align-content: start;
      min-width: 0;
      overflow: auto;
      padding: var(--space-4) var(--space-6);
    }

    .message-log {
      display: grid;
      align-content: start;
      min-width: 0;
      max-width: 116ch;
      margin: 0;
      padding: 0;
      list-style: none;
    }

    .message-row {
      display: grid;
      grid-template-columns: 5.5ch minmax(9ch, 16ch) minmax(0, 88ch);
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
      grid-template-columns: minmax(0, 90ch) auto;
      gap: var(--space-3);
      justify-content: start;
      padding: var(--space-4) var(--space-6) var(--space-5);
      border-top: 1px solid var(--border);
      background: var(--surface-raised);
    }

    .composer textarea {
      width: 100%;
      min-height: 48px;
      resize: vertical;
      padding: var(--space-3);
      line-height: 1.5;
    }

    .composer button {
      min-width: 84px;
      padding-inline: var(--space-4);
    }

    .empty-note,
    .inline-problem {
      max-width: 72ch;
      color: var(--text-muted);
      line-height: 1.55;
    }

    .inline-problem {
      margin-bottom: var(--space-3);
      padding: var(--space-3);
      border-left: 3px solid var(--danger);
      background: var(--surface-raised);
    }

    .older-button {
      justify-self: start;
      margin-bottom: var(--space-3);
      padding-inline: var(--space-4);
      color: var(--text);
      background: var(--surface-raised);
    }

    .visually-hidden {
      position: absolute;
      width: 1px;
      height: 1px;
      margin: -1px;
      overflow: hidden;
      clip: rect(0 0 0 0);
      white-space: nowrap;
    }

    @media (max-width: 760px) {
      .app-shell {
        grid-template-columns: 1fr;
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
      .chat-scroll,
      .composer {
        padding-inline: var(--space-4);
      }

      .chat-header {
        align-items: flex-start;
        flex-direction: column;
      }

      .chat-header h2 {
        white-space: normal;
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

      .composer textarea {
        width: 100%;
      }

      .composer button {
        min-height: 44px;
      }
    }
  `
}

function formDataString(form: HTMLFormElement, name: string): string {
  const value = new FormData(form).get(name)
  return typeof value === 'string' ? value : ''
}
