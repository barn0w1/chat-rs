import { LitElement, css, html } from 'lit'
import { customElement, property } from 'lit/decorators.js'
import type { RealtimeConnectionStatus } from '../realtime/socket'

@customElement('connection-status')
export class ConnectionStatus extends LitElement {
  @property()
  status: RealtimeConnectionStatus = 'idle'

  @property()
  message = ''

  render() {
    return html`
      <p class=${`status ${this.status}`} role="status">
        <span class="dot" aria-hidden="true"></span>
        <span>${this.label()}</span>
      </p>
    `
  }

  private label(): string {
    if (this.message !== '') {
      return this.message
    }

    switch (this.status) {
      case 'idle':
        return 'Realtime idle'
      case 'connecting':
        return 'Connecting realtime'
      case 'open':
        return 'Realtime connected'
      case 'reconnecting':
        return 'Realtime reconnecting'
      case 'closed':
        return 'Realtime closed'
      case 'error':
        return 'Realtime error'
    }
  }

  static styles = css`
    :host {
      display: block;
    }

    .status {
      display: inline-flex;
      align-items: center;
      gap: var(--space-2);
      margin: 0;
      color: var(--text-muted);
      font-size: 0.82rem;
      line-height: 1.35;
    }

    .dot {
      width: 8px;
      height: 8px;
      border: 1px solid var(--border-strong);
      border-radius: 999px;
      background: var(--surface-sunken);
    }

    .connecting .dot,
    .reconnecting .dot {
      background: var(--accent-soft);
    }

    .open .dot {
      background: var(--accent);
    }

    .error .dot {
      background: var(--danger);
    }
  `
}
