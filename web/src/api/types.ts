export type Id = string
export type MembershipRole = 'owner' | 'member'

export interface User {
  id: Id
  display_name: string
  created_at_ms: number
}

export interface Session {
  user: User
  csrf_token: string
}

export interface Conversation {
  id: Id
  title: string
  created_at_ms: number
  role: MembershipRole
}

export interface ConversationPage {
  conversations: Conversation[]
  next_cursor: Id | null
}

export interface Member {
  user: User
  role: MembershipRole
  joined_at_ms: number
}

export interface MemberPage {
  members: Member[]
  next_cursor: Id | null
}

export interface Message {
  id: Id
  conversation_id: Id
  author_id: Id
  body: string
  created_at_ms: number
}

export interface MessagePage {
  messages: Message[]
  next_cursor: Id | null
}

export interface PageQuery {
  before?: Id
  after?: Id
  limit?: number
}

export interface CreateConversationRequest {
  title: string
}

export interface PostMessageRequest {
  body: string
}
