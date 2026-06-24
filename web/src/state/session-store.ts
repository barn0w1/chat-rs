import { getSession, logout as requestLogout } from '../api/client'
import { ApiProblemError } from '../api/problems'
import type { ParsedProblem } from '../api/problems'
import type { Session } from '../api/types'

export type SessionStatus = 'idle' | 'loading' | 'authenticated' | 'unauthenticated' | 'error'

export interface SessionSnapshot {
  status: SessionStatus
  session?: Session
  problem?: ParsedProblem
  message?: string
}

const CHANGE_EVENT = 'change'

export class SessionStore extends EventTarget {
  private snapshot: SessionSnapshot = { status: 'idle' }
  private abortController?: AbortController
  private requestId = 0

  get current(): SessionSnapshot {
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
    this.set({ status: 'loading' })

    void getSession(controller.signal)
      .then((session) => {
        if (this.isStale(requestId)) {
          return
        }
        this.set({ status: 'authenticated', session })
      })
      .catch((error: unknown) => {
        if (this.isStale(requestId) || isAbortError(error)) {
          return
        }
        this.set(problemSnapshot(error))
      })
  }

  logout(): void {
    const csrfToken = this.snapshot.session?.csrf_token
    if (csrfToken === undefined) {
      this.clear()
      return
    }

    const requestId = this.nextRequest()
    const controller = new AbortController()
    this.abortController = controller
    this.set({ ...this.snapshot, status: 'loading' })

    void requestLogout(csrfToken, controller.signal)
      .then(() => {
        if (this.isStale(requestId)) {
          return
        }
        this.clear()
      })
      .catch((error: unknown) => {
        if (this.isStale(requestId) || isAbortError(error)) {
          return
        }
        this.set(problemSnapshot(error, this.snapshot.session))
      })
  }

  clear(): void {
    this.nextRequest()
    this.set({ status: 'unauthenticated' })
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

  private set(snapshot: SessionSnapshot): void {
    this.snapshot = snapshot
    this.dispatchEvent(new Event(CHANGE_EVENT))
  }
}

function problemSnapshot(error: unknown, session?: Session): SessionSnapshot {
  if (error instanceof ApiProblemError) {
    if (error.problem.category === 'authentication_required') {
      return { status: 'unauthenticated', problem: error.problem }
    }
    return {
      status: 'error',
      session,
      problem: error.problem,
      message: error.problem.title,
    }
  }

  return {
    status: 'error',
    session,
    message: 'Could not reach the server.',
  }
}

function isAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === 'AbortError'
}
