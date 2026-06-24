import type { Id } from '../api/types'

export type RealtimeServerEvent =
  | { type: 'ready' }
  | { type: 'subscribed'; conversation_id: Id }
  | { type: 'unsubscribed'; conversation_id: Id }
  | { type: 'subscription_rejected'; conversation_id: Id; reason: string }
  | { type: 'conversation_created'; conversation_id: Id }
  | { type: 'message_posted'; conversation_id: Id; message_id: Id }

export type RealtimeClientMessage =
  | { type: 'subscribe'; conversation_id: Id }
  | { type: 'unsubscribe'; conversation_id: Id }

export function parseRealtimeServerMessage(value: unknown): RealtimeServerEvent | undefined {
  if (!isRecord(value)) {
    return undefined
  }

  switch (value.type) {
    case 'ready':
      return { type: 'ready' }
    case 'subscribed': {
      const conversationId = stringField(value, 'conversation_id')
      return conversationId === undefined
        ? undefined
        : { type: 'subscribed', conversation_id: conversationId }
    }
    case 'unsubscribed': {
      const conversationId = stringField(value, 'conversation_id')
      return conversationId === undefined
        ? undefined
        : { type: 'unsubscribed', conversation_id: conversationId }
    }
    case 'subscription_rejected': {
      const conversationId = stringField(value, 'conversation_id')
      const reason = stringField(value, 'reason')
      if (conversationId === undefined || reason === undefined) {
        return undefined
      }
      return { type: 'subscription_rejected', conversation_id: conversationId, reason }
    }
    case 'conversation_created': {
      const conversationId = stringField(value, 'conversation_id')
      return conversationId === undefined
        ? undefined
        : { type: 'conversation_created', conversation_id: conversationId }
    }
    case 'message_posted': {
      const conversationId = stringField(value, 'conversation_id')
      const messageId = stringField(value, 'message_id')
      if (conversationId === undefined || messageId === undefined) {
        return undefined
      }
      return { type: 'message_posted', conversation_id: conversationId, message_id: messageId }
    }
    default:
      return undefined
  }
}

export function encodeRealtimeClientMessage(message: RealtimeClientMessage): string {
  return JSON.stringify(message)
}

export function parseJson(data: string): unknown {
  try {
    return JSON.parse(data)
  } catch {
    return undefined
  }
}

function stringField(record: Record<string, unknown>, key: string): string | undefined {
  const value = record[key]
  return typeof value === 'string' ? value : undefined
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}
