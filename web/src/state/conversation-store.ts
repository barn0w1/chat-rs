import { createConversation, listConversations } from '../api/client'
import { ApiProblemError } from '../api/problems'
import type { ParsedProblem } from '../api/problems'
import type { Conversation, Id } from '../api/types'

export type ConversationStatus = 'idle' | 'loading' | 'ready' | 'creating' | 'error'

export interface ConversationSnapshot {
  status: ConversationStatus
  conversations: Conversation[]
  selectedId?: Id
  nextCursor: Id | null
  problem?: ParsedProblem
  message?: string
}

const CHANGE_EVENT = 'change'
const FIRST_PAGE_LIMIT = 50

export class ConversationStore extends EventTarget {
  private snapshot: ConversationSnapshot = {
    status: 'idle',
    conversations: [],
    nextCursor: null,
  }

  private abortController?: AbortController
  private requestId = 0

  get current(): ConversationSnapshot {
    return this.snapshot
  }

  subscribe(listener: () => void): () => void {
    this.addEventListener(CHANGE_EVENT, listener)
    return () => this.removeEventListener(CHANGE_EVENT, listener)
  }

  load(): void {
    const requestId = this.nextRequest()
    const controller = new AbortController()
    this.abortController = controller
    this.set({ ...this.snapshot, status: 'loading', problem: undefined, message: undefined })

    void listConversations({ limit: FIRST_PAGE_LIMIT }, controller.signal)
      .then((page) => {
        if (this.isStale(requestId)) {
          return
        }
        this.set({
          status: 'ready',
          conversations: page.conversations,
          selectedId: selectedConversationId(page.conversations, this.snapshot.selectedId),
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

  select(conversationId: Id): void {
    if (!this.snapshot.conversations.some((conversation) => conversation.id === conversationId)) {
      return
    }

    this.set({
      ...this.snapshot,
      selectedId: conversationId,
      problem: undefined,
      message: undefined,
    })
  }

  create(title: string, csrfToken: string): void {
    const trimmedTitle = title.trim()
    if (trimmedTitle.length === 0) {
      this.set({
        ...this.snapshot,
        status: 'error',
        message: 'Conversation title is required.',
      })
      return
    }

    const requestId = this.nextRequest()
    const controller = new AbortController()
    this.abortController = controller
    this.set({ ...this.snapshot, status: 'creating', problem: undefined, message: undefined })

    void createConversation({ title: trimmedTitle }, csrfToken, controller.signal)
      .then((conversation) => {
        if (this.isStale(requestId)) {
          return
        }
        this.set({
          status: 'ready',
          conversations: mergeConversation(this.snapshot.conversations, conversation),
          selectedId: conversation.id,
          nextCursor: this.snapshot.nextCursor,
        })
      })
      .catch((error: unknown) => {
        if (this.isStale(requestId) || isAbortError(error)) {
          return
        }
        this.set(problemSnapshot(error, this.snapshot))
      })
  }

  clear(): void {
    this.nextRequest()
    this.set({ status: 'idle', conversations: [], nextCursor: null })
  }

  dispose(): void {
    this.abortController?.abort()
  }

  private nextRequest(): number {
    this.abortController?.abort()
    this.requestId += 1
    return this.requestId
  }

  private isStale(requestId: number): boolean {
    return requestId !== this.requestId
  }

  private set(snapshot: ConversationSnapshot): void {
    this.snapshot = snapshot
    this.dispatchEvent(new Event(CHANGE_EVENT))
  }
}

function selectedConversationId(conversations: Conversation[], currentId?: Id): Id | undefined {
  if (currentId !== undefined && conversations.some((conversation) => conversation.id === currentId)) {
    return currentId
  }
  return conversations[0]?.id
}

function mergeConversation(conversations: Conversation[], created: Conversation): Conversation[] {
  return [
    created,
    ...conversations.filter((conversation) => conversation.id !== created.id),
  ]
}

function problemSnapshot(
  error: unknown,
  current: ConversationSnapshot,
): ConversationSnapshot {
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

function isAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === 'AbortError'
}
