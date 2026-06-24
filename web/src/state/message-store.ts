import { getMessage, listMessages, postMessage } from '../api/client'
import { ApiProblemError } from '../api/problems'
import type { ParsedProblem } from '../api/problems'
import type { Id, Message } from '../api/types'

export type MessageStatus = 'idle' | 'loading' | 'ready' | 'loading_older' | 'posting' | 'error' | 'unknown'

export interface MessageSnapshot {
  status: MessageStatus
  conversationId?: Id
  messages: Message[]
  nextCursor: Id | null
  problem?: ParsedProblem
  message?: string
}

const CHANGE_EVENT = 'change'
const FIRST_PAGE_LIMIT = 50
const OLDER_PAGE_LIMIT = 50

export class MessageStore extends EventTarget {
  private snapshot: MessageSnapshot = {
    status: 'idle',
    messages: [],
    nextCursor: null,
  }

  private abortController?: AbortController
  private singleMessageController?: AbortController
  private requestId = 0
  private singleMessageRequestId = 0

  get current(): MessageSnapshot {
    return this.snapshot
  }

  subscribe(listener: () => void): () => void {
    this.addEventListener(CHANGE_EVENT, listener)
    return () => this.removeEventListener(CHANGE_EVENT, listener)
  }

  load(conversationId: Id): void {
    const requestId = this.nextRequest()
    const controller = new AbortController()
    this.abortController = controller
    this.set({
      status: 'loading',
      conversationId,
      messages: [],
      nextCursor: null,
    })

    void listMessages(conversationId, { limit: FIRST_PAGE_LIMIT }, controller.signal)
      .then((page) => {
        if (this.isStale(requestId)) {
          return
        }
        this.set({
          status: 'ready',
          conversationId,
          messages: newestFirstPageToLogOrder(page.messages),
          nextCursor: page.next_cursor,
        })
      })
      .catch((error: unknown) => {
        if (this.isStale(requestId) || isAbortError(error)) {
          return
        }
        this.set(problemSnapshot(error, this.snapshot))
      })
  }

  loadOlder(): void {
    const conversationId = this.snapshot.conversationId
    const before = this.snapshot.nextCursor
    if (conversationId === undefined || before === null) {
      return
    }

    const requestId = this.nextRequest()
    const controller = new AbortController()
    this.abortController = controller
    this.set({ ...this.snapshot, status: 'loading_older', problem: undefined, message: undefined })

    void listMessages(conversationId, { before, limit: OLDER_PAGE_LIMIT }, controller.signal)
      .then((page) => {
        if (this.isStale(requestId)) {
          return
        }
        this.set({
          status: 'ready',
          conversationId,
          messages: mergeOlderMessages(this.snapshot.messages, page.messages),
          nextCursor: page.next_cursor,
        })
      })
      .catch((error: unknown) => {
        if (this.isStale(requestId) || isAbortError(error)) {
          return
        }
        this.set(problemSnapshot(error, this.snapshot))
      })
  }

  post(conversationId: Id, body: string, csrfToken: string): void {
    const trimmedBody = body.trim()
    if (trimmedBody.length === 0) {
      this.set({
        ...this.snapshot,
        status: 'error',
        message: 'Message body is required.',
      })
      return
    }

    const requestId = this.nextRequest()
    const controller = new AbortController()
    this.abortController = controller
    const currentMessages =
      this.snapshot.conversationId === conversationId ? this.snapshot.messages : []

    this.set({
      ...this.snapshot,
      status: 'posting',
      conversationId,
      messages: currentMessages,
      problem: undefined,
      message: undefined,
    })

    void postMessage(conversationId, { body: trimmedBody }, csrfToken, controller.signal)
      .then((message) => {
        if (this.isStale(requestId)) {
          return
        }
        this.set({
          status: 'ready',
          conversationId,
          messages: mergeNewMessage(this.snapshot.messages, message),
          nextCursor: this.snapshot.nextCursor,
        })
      })
      .catch((error: unknown) => {
        if (this.isStale(requestId) || isAbortError(error)) {
          return
        }
        this.set(postFailureSnapshot(error, this.snapshot))
      })
  }

  fetchOne(conversationId: Id, messageId: Id): void {
    if (this.snapshot.conversationId !== conversationId) {
      return
    }

    this.singleMessageController?.abort()
    this.singleMessageRequestId += 1
    const requestId = this.singleMessageRequestId
    const controller = new AbortController()
    this.singleMessageController = controller

    void getMessage(conversationId, messageId, controller.signal)
      .then((message) => {
        if (this.isStaleSingleMessage(requestId) || this.snapshot.conversationId !== conversationId) {
          return
        }
        this.set({
          ...this.snapshot,
          messages: mergeNewMessage(this.snapshot.messages, message),
          message: undefined,
          problem: undefined,
        })
      })
      .catch((error: unknown) => {
        if (this.isStaleSingleMessage(requestId) || isAbortError(error)) {
          return
        }
        this.set(realtimeFetchFailureSnapshot(error, this.snapshot))
      })
  }

  clear(): void {
    this.nextRequest()
    this.nextSingleMessageRequest()
    this.set({ status: 'idle', messages: [], nextCursor: null })
  }

  dispose(): void {
    this.abortController?.abort()
    this.singleMessageController?.abort()
  }

  private nextRequest(): number {
    this.abortController?.abort()
    this.requestId += 1
    return this.requestId
  }

  private isStale(requestId: number): boolean {
    return requestId !== this.requestId
  }

  private nextSingleMessageRequest(): number {
    this.singleMessageController?.abort()
    this.singleMessageRequestId += 1
    return this.singleMessageRequestId
  }

  private isStaleSingleMessage(requestId: number): boolean {
    return requestId !== this.singleMessageRequestId
  }

  private set(snapshot: MessageSnapshot): void {
    this.snapshot = snapshot
    this.dispatchEvent(new Event(CHANGE_EVENT))
  }
}

function newestFirstPageToLogOrder(messages: Message[]): Message[] {
  return [...messages].reverse()
}

function mergeOlderMessages(current: Message[], olderNewestFirst: Message[]): Message[] {
  const existingIds = new Set(current.map((message) => message.id))
  const older = newestFirstPageToLogOrder(olderNewestFirst).filter(
    (message) => !existingIds.has(message.id),
  )
  return [...older, ...current]
}

function mergeNewMessage(current: Message[], message: Message): Message[] {
  if (current.some((entry) => entry.id === message.id)) {
    return current
  }
  return [...current, message]
}

function problemSnapshot(error: unknown, current: MessageSnapshot): MessageSnapshot {
  if (error instanceof ApiProblemError) {
    return {
      ...current,
      status: 'error',
      problem: error.problem,
      message: error.problem.title,
    }
  }

  return {
    ...current,
    status: 'error',
    message: 'Could not reach the server. Refresh before retrying.',
  }
}

function postFailureSnapshot(error: unknown, current: MessageSnapshot): MessageSnapshot {
  if (error instanceof ApiProblemError) {
    return problemSnapshot(error, current)
  }

  return {
    ...current,
    status: 'unknown',
    message: 'The message result is unknown. Refresh before sending it again.',
  }
}

function realtimeFetchFailureSnapshot(error: unknown, current: MessageSnapshot): MessageSnapshot {
  if (error instanceof ApiProblemError) {
    return {
      ...current,
      problem: error.problem,
      message: `Realtime message refresh failed: ${error.problem.title}`,
    }
  }

  return {
    ...current,
    message: 'Realtime message refresh failed. Refresh to retry.',
  }
}

function isAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === 'AbortError'
}
