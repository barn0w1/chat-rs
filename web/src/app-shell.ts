import { LitElement, css, html } from 'lit'
import { customElement } from 'lit/decorators.js'

const previewConversations = [
  {
    id: '42',
    title: 'General',
    meta: 'API client ready',
    selected: true,
  },
  {
    id: '41',
    title: 'Operations',
    meta: 'Session store next',
    selected: false,
  },
]

const previewMessages = [
  {
    id: '99',
    author: 'chat-rs',
    body: 'Typed HTTP boundaries are ready. The next step can load the session and replace this preview state.',
    time: 'Now',
    mine: false,
  },
  {
    id: '98',
    author: 'Yuito',
    body: 'Text first, light, blocky, and explicit.',
    time: 'Earlier',
    mine: true,
  },
]

@customElement('chat-app')
export class ChatApp extends LitElement {
  render() {
    return html`
      <main class="app-shell" aria-label="chat-rs web client">
        <aside class="sidebar" aria-label="Conversations">
          <header class="sidebar-header">
            <div>
              <p class="eyebrow">chat-rs</p>
              <h1>Conversations</h1>
            </div>
            <button class="icon-button" type="button" aria-label="Create conversation" disabled>
              +
            </button>
          </header>

          <div class="status-line" role="status">
            <span class="status-dot" aria-hidden="true"></span>
            Static preview
          </div>

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
        </aside>

        <section class="chat-panel" aria-label="Selected conversation">
          <header class="chat-header">
            <div>
              <p class="eyebrow">General</p>
              <h2>Ready for session loading</h2>
            </div>
            <div class="session-pill">No session loaded</div>
          </header>

          <ol class="message-list" aria-label="Message history">
            ${previewMessages.map(
              (message) => html`
                <li
                  class=${message.mine ? 'message mine' : 'message'}
                  data-message-id=${message.id}
                >
                  <div class="message-meta">
                    <span>${message.author}</span>
                    <time>${message.time}</time>
                  </div>
                  <p>${message.body}</p>
                </li>
              `,
            )}
          </ol>

          <form class="composer" aria-label="Message composer">
            <textarea
              rows="2"
              placeholder="Session and message posting arrive in later steps."
              disabled
            ></textarea>
            <button type="submit" disabled>Send</button>
          </form>
        </section>
      </main>
    `
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
      border-inline: 2px solid var(--border-strong);
      background: var(--surface);
      box-shadow: var(--shadow);
    }

    .sidebar {
      display: flex;
      min-width: 0;
      flex-direction: column;
      gap: var(--space-4);
      padding: var(--space-5);
      border-right: 2px solid var(--border-strong);
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
      font-size: 1.45rem;
      line-height: 1.15;
    }

    h2 {
      margin-top: var(--space-1);
      font-size: 1.25rem;
      line-height: 1.2;
    }

    .eyebrow,
    .conversation-meta,
    .message-meta,
    .session-pill,
    .status-line {
      color: var(--text-muted);
      font-size: 0.82rem;
      line-height: 1.3;
    }

    .eyebrow {
      font-weight: 700;
      letter-spacing: 0.08em;
      text-transform: uppercase;
    }

    .icon-button,
    .composer button {
      border: 2px solid var(--border-strong);
      border-radius: var(--radius-md);
      color: var(--surface-raised);
      background: var(--accent);
      font-weight: 700;
      box-shadow: inset -2px -2px 0 rgb(0 0 0 / 22%), inset 2px 2px 0 rgb(255 255 255 / 24%);
    }

    .icon-button {
      width: 36px;
      height: 36px;
      flex: 0 0 auto;
      font-size: 1.3rem;
      line-height: 1;
    }

    .icon-button:disabled,
    .composer button:disabled {
      color: var(--text-muted);
      background: var(--accent-soft);
      border-color: var(--border);
      box-shadow: inset -2px -2px 0 rgb(0 0 0 / 12%), inset 2px 2px 0 rgb(255 255 255 / 22%);
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
      border-radius: 0;
      background: var(--accent);
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
      border: 2px solid transparent;
      border-radius: var(--radius-md);
      color: var(--text);
      background: transparent;
      text-align: left;
    }

    .conversation.selected {
      border-color: var(--border-strong);
      background: var(--surface-raised);
      box-shadow: inset -2px -2px 0 rgb(0 0 0 / 12%), inset 2px 2px 0 rgb(255 255 255 / 40%);
    }

    .conversation-title {
      overflow: hidden;
      font-weight: 700;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .chat-panel {
      display: grid;
      min-width: 0;
      grid-template-rows: auto minmax(0, 1fr) auto;
      background: var(--surface);
    }

    .chat-header {
      min-width: 0;
      padding: var(--space-5) var(--space-6);
      border-bottom: 2px solid var(--border-strong);
    }

    .session-pill {
      flex: 0 0 auto;
      padding: 6px 10px;
      border: 2px solid var(--border);
      border-radius: 999px;
      background: var(--surface-muted);
    }

    .message-list {
      display: flex;
      min-width: 0;
      flex-direction: column;
      gap: var(--space-3);
      margin: 0;
      padding: var(--space-6);
      overflow: auto;
      list-style: none;
    }

    .message {
      width: min(72ch, 86%);
      padding: var(--space-3) var(--space-4);
      border: 2px solid var(--border-strong);
      border-radius: var(--radius-md);
      background: var(--surface-raised);
      box-shadow: inset -2px -2px 0 rgb(0 0 0 / 10%), inset 2px 2px 0 rgb(255 255 255 / 44%);
    }

    .message.mine {
      align-self: flex-end;
      background: var(--accent-soft);
    }

    .message-meta {
      display: flex;
      justify-content: space-between;
      gap: var(--space-3);
      margin-bottom: var(--space-2);
      font-family: var(--font-mono);
    }

    .message p {
      line-height: 1.5;
    }

    .composer {
      display: grid;
      grid-template-columns: minmax(0, 1fr) auto;
      gap: var(--space-3);
      padding: var(--space-4) var(--space-6) var(--space-5);
      border-top: 2px solid var(--border-strong);
      background: var(--surface);
    }

    .composer textarea {
      width: 100%;
      min-height: 48px;
      resize: vertical;
      border: 2px solid var(--border);
      border-radius: var(--radius-md);
      padding: var(--space-3);
      color: var(--text);
      background: var(--surface-raised);
      box-shadow: inset 2px 2px 0 rgb(0 0 0 / 12%);
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
        border-bottom: 2px solid var(--border-strong);
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
      .message-list,
      .composer {
        padding-inline: var(--space-4);
      }

      .chat-header {
        align-items: flex-start;
        flex-direction: column;
      }

      .message {
        width: 100%;
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
