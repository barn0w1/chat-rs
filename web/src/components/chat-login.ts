import { LitElement, css, html } from 'lit'
import { customElement, property } from 'lit/decorators.js'

@customElement('chat-login')
export class ChatLogin extends LitElement {
  @property({ attribute: 'status-message' })
  statusMessage = ''

  render() {
    return html`
      <section class="login-panel" aria-labelledby="login-title">
        <div>
          <p class="eyebrow">session</p>
          <h2 id="login-title">Sign in to chat-rs</h2>
          <p class="lede">
            Use the server's OpenID Connect login. If this server is invite-only,
            enter an admission code before continuing.
          </p>
        </div>

        ${this.statusMessage === ''
          ? null
          : html`<p class="status" role="status">${this.statusMessage}</p>`}

        <div class="actions">
          <a class="primary-action" href="/auth/oidc/start">Sign in</a>
        </div>

        <form class="admission-form" action="/auth/oidc/start" method="post">
          <label for="admission-code">Admission code</label>
          <div class="admission-row">
            <input
              id="admission-code"
              name="admission_code"
              type="text"
              autocomplete="one-time-code"
              spellcheck="false"
              inputmode="text"
            />
            <button type="submit">Sign in with code</button>
          </div>
        </form>

        <button class="secondary-action" type="button" @click=${this.emitRetry}>
          Retry session check
        </button>
      </section>
    `
  }

  private emitRetry() {
    this.dispatchEvent(new CustomEvent('retry-session', { bubbles: true, composed: true }))
  }

  static styles = css`
    :host {
      display: block;
    }

    .login-panel {
      display: grid;
      max-width: 620px;
      gap: var(--space-5);
      padding: var(--space-6);
      border: 1px solid var(--border);
      background: var(--surface-raised);
    }

    h2,
    p {
      margin: 0;
    }

    h2 {
      margin-top: var(--space-1);
      font-size: 1.25rem;
      line-height: 1.25;
    }

    .eyebrow {
      color: var(--text-muted);
      font-family: var(--font-mono);
      font-size: 0.78rem;
      letter-spacing: 0.08em;
      text-transform: uppercase;
    }

    .lede,
    .status {
      color: var(--text-muted);
      line-height: 1.55;
    }

    .status {
      padding: var(--space-3);
      border-left: 3px solid var(--border-strong);
      background: var(--surface-muted);
    }

    .actions,
    .admission-row {
      display: flex;
      flex-wrap: wrap;
      gap: var(--space-3);
    }

    .primary-action,
    button {
      min-height: 40px;
      border: 1px solid var(--border-strong);
      color: var(--surface-raised);
      background: var(--accent);
      font-weight: 700;
      text-decoration: none;
    }

    .primary-action {
      display: inline-flex;
      align-items: center;
      padding: 0 var(--space-4);
    }

    .secondary-action {
      justify-self: start;
      color: var(--text);
      background: var(--surface);
    }

    .admission-form {
      display: grid;
      gap: var(--space-2);
    }

    label {
      color: var(--text-muted);
      font-family: var(--font-mono);
      font-size: 0.86rem;
    }

    input {
      min-width: min(100%, 260px);
      min-height: 40px;
      border: 1px solid var(--border);
      padding: 0 var(--space-3);
      color: var(--text);
      background: var(--surface);
      font: inherit;
    }

    button {
      padding: 0 var(--space-4);
      cursor: pointer;
    }

    @media (max-width: 640px) {
      .login-panel {
        padding: var(--space-4);
      }

      .primary-action,
      .admission-row input,
      .admission-row button {
        width: 100%;
      }
    }
  `
}
