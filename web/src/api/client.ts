import { ApiProblemError, parseProblemDocument } from './problems'
import type {
  Conversation,
  ConversationPage,
  CreateConversationRequest,
  Id,
  MemberPage,
  Message,
  MessagePage,
  PageQuery,
  PostMessageRequest,
  Session,
} from './types'

const CSRF_HEADER = 'X-CSRF-Token'

interface JsonRequestOptions {
  method?: 'GET' | 'POST' | 'DELETE'
  csrfToken?: string
  body?: unknown
  signal?: AbortSignal
  expectedStatus?: number
}

export class ApiClientError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'ApiClientError'
  }
}

export function getSession(signal?: AbortSignal): Promise<Session> {
  return requestJson<Session>('/api/v1/session', { signal })
}

export async function logout(csrfToken: string, signal?: AbortSignal): Promise<void> {
  const response = await request('/api/v1/session', {
    method: 'DELETE',
    csrfToken,
    signal,
  })
  if (response.status !== 204) {
    await throwProblem(response)
  }
}

export function listConversations(
  query: Pick<PageQuery, 'before' | 'limit'> = {},
  signal?: AbortSignal,
): Promise<ConversationPage> {
  return requestJson<ConversationPage>(buildUrl('/api/v1/conversations', query), { signal })
}

export function createConversation(
  requestBody: CreateConversationRequest,
  csrfToken: string,
  signal?: AbortSignal,
): Promise<Conversation> {
  return requestJson<Conversation>('/api/v1/conversations', {
    method: 'POST',
    body: requestBody,
    csrfToken,
    signal,
    expectedStatus: 201,
  })
}

export function getConversation(conversationId: Id, signal?: AbortSignal): Promise<Conversation> {
  return requestJson<Conversation>(`/api/v1/conversations/${encodePathSegment(conversationId)}`, {
    signal,
  })
}

export function listMembers(
  conversationId: Id,
  query: Pick<PageQuery, 'after' | 'limit'> = {},
  signal?: AbortSignal,
): Promise<MemberPage> {
  return requestJson<MemberPage>(
    buildUrl(`/api/v1/conversations/${encodePathSegment(conversationId)}/members`, query),
    { signal },
  )
}

export function listMessages(
  conversationId: Id,
  query: Pick<PageQuery, 'before' | 'limit'> = {},
  signal?: AbortSignal,
): Promise<MessagePage> {
  return requestJson<MessagePage>(
    buildUrl(`/api/v1/conversations/${encodePathSegment(conversationId)}/messages`, query),
    { signal },
  )
}

export function postMessage(
  conversationId: Id,
  requestBody: PostMessageRequest,
  csrfToken: string,
  signal?: AbortSignal,
): Promise<Message> {
  return requestJson<Message>(
    `/api/v1/conversations/${encodePathSegment(conversationId)}/messages`,
    {
      method: 'POST',
      body: requestBody,
      csrfToken,
      signal,
      expectedStatus: 201,
    },
  )
}

export function getMessage(
  conversationId: Id,
  messageId: Id,
  signal?: AbortSignal,
): Promise<Message> {
  return requestJson<Message>(
    `/api/v1/conversations/${encodePathSegment(conversationId)}/messages/${encodePathSegment(
      messageId,
    )}`,
    { signal },
  )
}

async function requestJson<T>(path: string, options: JsonRequestOptions = {}): Promise<T> {
  const response = await request(path, options)
  const expectedStatus = options.expectedStatus ?? 200
  if (response.status !== expectedStatus) {
    await throwProblem(response)
  }

  const contentType = response.headers.get('content-type')?.toLowerCase() ?? ''
  if (!contentType.includes('application/json')) {
    throw new ApiClientError(`Expected JSON response for ${path}`)
  }

  const body = await readJson(response)
  if (body === undefined) {
    throw new ApiClientError(`Invalid JSON response for ${path}`)
  }

  return body as T
}

function request(path: string, options: JsonRequestOptions): Promise<Response> {
  const headers = new Headers()
  headers.set('Accept', 'application/json')

  if (options.body !== undefined) {
    headers.set('Content-Type', 'application/json')
  }
  if (options.csrfToken !== undefined) {
    headers.set(CSRF_HEADER, options.csrfToken)
  }

  return fetch(path, {
    method: options.method ?? 'GET',
    credentials: 'same-origin',
    headers,
    body: options.body === undefined ? undefined : JSON.stringify(options.body),
    signal: options.signal,
  })
}

async function throwProblem(response: Response): Promise<never> {
  const contentType = response.headers.get('content-type')?.toLowerCase() ?? ''
  if (contentType.includes('application/problem+json') || contentType.includes('application/json')) {
    throw new ApiProblemError(parseProblemDocument(await readJson(response), response.status))
  }

  throw new ApiProblemError(parseProblemDocument(undefined, response.status))
}

async function readJson(response: Response): Promise<unknown | undefined> {
  try {
    return await response.json()
  } catch {
    return undefined
  }
}

function buildUrl(path: string, query: PageQuery): string {
  const params = new URLSearchParams()
  if (query.before !== undefined) {
    params.set('before', query.before)
  }
  if (query.after !== undefined) {
    params.set('after', query.after)
  }
  if (query.limit !== undefined) {
    params.set('limit', query.limit.toString())
  }

  const encoded = params.toString()
  return encoded.length === 0 ? path : `${path}?${encoded}`
}

function encodePathSegment(value: Id): string {
  return encodeURIComponent(value)
}
