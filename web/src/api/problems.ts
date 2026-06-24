export type ProblemCategory =
  | 'authentication_required'
  | 'forbidden'
  | 'invalid_request'
  | 'validation_failed'
  | 'content_too_large'
  | 'unsupported_media_type'
  | 'not_found'
  | 'login_failed'
  | 'service_unavailable'
  | 'internal'
  | 'unexpected'

export interface FieldError {
  field: string
  code: string
  max?: number
}

export interface ProblemDocument {
  type: string
  title: string
  status: number
  errors?: FieldError[]
}

export interface ParsedProblem {
  category: ProblemCategory
  type: string
  title: string
  status: number
  errors: FieldError[]
}

const TYPE_CATEGORIES: Record<string, ProblemCategory> = {
  'urn:chat-rs:problem:authentication-required': 'authentication_required',
  'urn:chat-rs:problem:forbidden': 'forbidden',
  'urn:chat-rs:problem:invalid-request': 'invalid_request',
  'urn:chat-rs:problem:validation-failed': 'validation_failed',
  'urn:chat-rs:problem:content-too-large': 'content_too_large',
  'urn:chat-rs:problem:unsupported-media-type': 'unsupported_media_type',
  'urn:chat-rs:problem:not-found': 'not_found',
  'urn:chat-rs:problem:login-failed': 'login_failed',
  'urn:chat-rs:problem:service-unavailable': 'service_unavailable',
  'urn:chat-rs:problem:internal': 'internal',
}

export class ApiProblemError extends Error {
  readonly problem: ParsedProblem

  constructor(problem: ParsedProblem) {
    super(problem.title)
    this.name = 'ApiProblemError'
    this.problem = problem
  }
}

export function parseProblemDocument(value: unknown, fallbackStatus: number): ParsedProblem {
  if (!isRecord(value)) {
    return fallbackProblem(fallbackStatus)
  }

  const type = stringField(value, 'type') ?? 'about:blank'
  const title = stringField(value, 'title') ?? fallbackTitle(fallbackStatus)
  const status = numberField(value, 'status') ?? fallbackStatus
  const errors = parseFieldErrors(value.errors)

  return {
    category: categoryFor(type, status),
    type,
    title,
    status,
    errors,
  }
}

export function categoryFor(type: string, status: number): ProblemCategory {
  const known = TYPE_CATEGORIES[type]
  if (known !== undefined) {
    return known
  }

  switch (status) {
    case 400:
      return 'invalid_request'
    case 401:
      return 'authentication_required'
    case 403:
      return 'forbidden'
    case 404:
      return 'not_found'
    case 413:
      return 'content_too_large'
    case 415:
      return 'unsupported_media_type'
    case 422:
      return 'validation_failed'
    case 500:
      return 'internal'
    case 503:
      return 'service_unavailable'
    default:
      return 'unexpected'
  }
}

function fallbackProblem(status: number): ParsedProblem {
  return {
    category: categoryFor('about:blank', status),
    type: 'about:blank',
    title: fallbackTitle(status),
    status,
    errors: [],
  }
}

function fallbackTitle(status: number): string {
  if (status >= 500) {
    return 'Server error'
  }
  if (status >= 400) {
    return 'Request failed'
  }
  return 'Unexpected response'
}

function parseFieldErrors(value: unknown): FieldError[] {
  if (!Array.isArray(value)) {
    return []
  }

  return value.flatMap((entry) => {
    if (!isRecord(entry)) {
      return []
    }

    const field = stringField(entry, 'field')
    const code = stringField(entry, 'code')
    if (field === undefined || code === undefined) {
      return []
    }

    const max = numberField(entry, 'max')
    return max === undefined ? [{ field, code }] : [{ field, code, max }]
  })
}

function stringField(record: Record<string, unknown>, key: string): string | undefined {
  const value = record[key]
  return typeof value === 'string' ? value : undefined
}

function numberField(record: Record<string, unknown>, key: string): number | undefined {
  const value = record[key]
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}
