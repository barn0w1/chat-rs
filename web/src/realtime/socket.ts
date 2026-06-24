import type { Id } from '../api/types'
import {
  encodeRealtimeClientMessage,
  parseJson,
  parseRealtimeServerMessage,
} from './protocol'
import type { RealtimeServerEvent } from './protocol'

export type RealtimeConnectionStatus =
  | 'idle'
  | 'connecting'
  | 'open'
  | 'reconnecting'
  | 'closed'
  | 'error'

export interface RealtimeSnapshot {
  status: RealtimeConnectionStatus
  subscribedId?: Id
  message?: string
  attempt: number
}

const CHANGE_EVENT = 'change'
const SERVER_EVENT = 'server-event'
const SOCKET_PATH = '/api/v1/ws'
const SOCKET_PROTOCOL = 'chat.v1'
const INITIAL_BACKOFF_MS = 500
const MAX_BACKOFF_MS = 10_000

export class RealtimeSocket extends EventTarget {
  private snapshot: RealtimeSnapshot = { status: 'idle', attempt: 0 }
  private socket?: WebSocket
  private reconnectTimer?: number
  private shouldReconnect = false
  private desiredSubscriptionId?: Id
  private activeSubscriptionId?: Id
  private ready = false

  get current(): RealtimeSnapshot {
    return this.snapshot
  }

  subscribe(listener: () => void): () => void {
    this.addEventListener(CHANGE_EVENT, listener)
    return () => this.removeEventListener(CHANGE_EVENT, listener)
  }

  subscribeEvents(listener: (event: RealtimeServerEvent) => void): () => void {
    const eventListener = (event: Event) => {
      listener((event as CustomEvent<RealtimeServerEvent>).detail)
    }
    this.addEventListener(SERVER_EVENT, eventListener)
    return () => this.removeEventListener(SERVER_EVENT, eventListener)
  }

  start(): void {
    if (this.shouldReconnect && this.socket !== undefined) {
      return
    }

    this.shouldReconnect = true
    this.connect(0)
  }

  stop(): void {
    this.shouldReconnect = false
    this.clearReconnectTimer()
    this.ready = false
    this.activeSubscriptionId = undefined
    this.desiredSubscriptionId = undefined
    this.socket?.close()
    this.socket = undefined
    this.set({ status: 'closed', subscribedId: undefined, attempt: 0 })
  }

  setSubscription(conversationId?: Id): void {
    if (this.desiredSubscriptionId === conversationId) {
      return
    }

    const previousId = this.desiredSubscriptionId
    this.desiredSubscriptionId = conversationId

    if (this.isOpenAndReady()) {
      if (previousId !== undefined) {
        this.send({ type: 'unsubscribe', conversation_id: previousId })
      }
      if (conversationId !== undefined) {
        this.send({ type: 'subscribe', conversation_id: conversationId })
      }
    }
  }

  private connect(attempt: number): void {
    this.clearReconnectTimer()
    this.ready = false
    this.activeSubscriptionId = undefined
    this.socket?.close()

    const socket = new WebSocket(realtimeUrl(), SOCKET_PROTOCOL)
    this.socket = socket
    this.set({
      status: attempt === 0 ? 'connecting' : 'reconnecting',
      subscribedId: undefined,
      attempt,
    })

    socket.addEventListener('open', () => {
      this.set({ status: 'open', subscribedId: undefined, attempt })
    })

    socket.addEventListener('message', (event) => {
      if (typeof event.data !== 'string') {
        return
      }

      const parsed = parseRealtimeServerMessage(parseJson(event.data))
      if (parsed === undefined) {
        return
      }

      this.handleServerEvent(parsed)
    })

    socket.addEventListener('close', () => {
      if (this.socket !== socket) {
        return
      }
      this.socket = undefined
      this.ready = false
      this.activeSubscriptionId = undefined

      if (!this.shouldReconnect) {
        this.set({ status: 'closed', subscribedId: undefined, attempt: 0 })
        return
      }

      this.scheduleReconnect(attempt + 1)
    })

    socket.addEventListener('error', () => {
      if (this.socket !== socket) {
        return
      }
      this.set({
        status: 'error',
        subscribedId: this.activeSubscriptionId,
        message: 'Realtime connection failed.',
        attempt,
      })
    })
  }

  private handleServerEvent(event: RealtimeServerEvent): void {
    switch (event.type) {
      case 'ready':
        this.ready = true
        this.sendDesiredSubscription()
        break
      case 'subscribed':
        this.activeSubscriptionId = event.conversation_id
        this.set({
          ...this.snapshot,
          subscribedId: event.conversation_id,
          message: undefined,
        })
        break
      case 'unsubscribed':
        if (this.activeSubscriptionId === event.conversation_id) {
          this.activeSubscriptionId = undefined
          this.set({ ...this.snapshot, subscribedId: undefined, message: undefined })
        }
        break
      case 'subscription_rejected':
        if (this.desiredSubscriptionId === event.conversation_id) {
          this.activeSubscriptionId = undefined
          this.set({
            ...this.snapshot,
            subscribedId: undefined,
            message: `Realtime subscription rejected: ${event.reason}`,
          })
        }
        break
      case 'conversation_created':
      case 'message_posted':
        break
    }

    this.dispatchEvent(new CustomEvent<RealtimeServerEvent>(SERVER_EVENT, { detail: event }))
  }

  private scheduleReconnect(attempt: number): void {
    const delay = Math.min(INITIAL_BACKOFF_MS * 2 ** Math.max(0, attempt - 1), MAX_BACKOFF_MS)
    this.set({
      status: 'reconnecting',
      subscribedId: undefined,
      message: `Reconnecting in ${Math.round(delay / 1000)}s.`,
      attempt,
    })
    this.reconnectTimer = window.setTimeout(() => this.connect(attempt), delay)
  }

  private sendDesiredSubscription(): void {
    if (this.desiredSubscriptionId !== undefined) {
      this.send({ type: 'subscribe', conversation_id: this.desiredSubscriptionId })
    }
  }

  private send(message: Parameters<typeof encodeRealtimeClientMessage>[0]): void {
    if (!this.isSocketOpen()) {
      return
    }
    this.socket?.send(encodeRealtimeClientMessage(message))
  }

  private isOpenAndReady(): boolean {
    return this.ready && this.isSocketOpen()
  }

  private isSocketOpen(): boolean {
    return this.socket?.readyState === WebSocket.OPEN
  }

  private clearReconnectTimer(): void {
    if (this.reconnectTimer !== undefined) {
      window.clearTimeout(this.reconnectTimer)
      this.reconnectTimer = undefined
    }
  }

  private set(snapshot: RealtimeSnapshot): void {
    this.snapshot = snapshot
    this.dispatchEvent(new Event(CHANGE_EVENT))
  }
}

function realtimeUrl(): string {
  const url = new URL(SOCKET_PATH, window.location.href)
  url.protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  return url.toString()
}
