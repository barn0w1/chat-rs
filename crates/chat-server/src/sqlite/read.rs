use chat::{
    ConversationDetails, ConversationPage, GetConversationError, GetConversationStore,
    ListConversations, ListConversationsError, ListConversationsStore, ListMembers,
    ListMembersError, ListMembersStore, ListMessages, ListMessagesError, ListMessagesStore,
    MAX_CONVERSATION_PAGE_SIZE, MAX_MEMBER_PAGE_SIZE, MAX_MESSAGE_PAGE_SIZE, MemberPage,
    MessagePage, UserId,
};

use super::{
    SqliteStore,
    row::{ConversationWithRoleRow, OptionalConversationMemberRow, OptionalMessageRow},
};

impl GetConversationStore for SqliteStore {
    async fn get_conversation(
        &self,
        actor_id: UserId,
        conversation_id: chat::ConversationId,
    ) -> Result<ConversationDetails, GetConversationError> {
        let row = sqlx::query_as::<_, ConversationWithRoleRow>(
            "SELECT c.id, c.title, c.created_at_ms, membership.role \
             FROM conversations AS c \
             JOIN conversation_members AS membership ON membership.conversation_id = c.id \
             WHERE c.id = ? AND membership.user_id = ?",
        )
        .bind(conversation_id.get())
        .bind(actor_id.get())
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| GetConversationError::StoreUnavailable)?
        .ok_or(GetConversationError::NotFound)?;

        row.into_details()
            .map_err(|_| GetConversationError::InvalidStoreResult)
    }
}

impl ListConversationsStore for SqliteStore {
    async fn list_conversations(
        &self,
        actor_id: UserId,
        query: ListConversations,
    ) -> Result<ConversationPage, ListConversationsError> {
        if !(1..=MAX_CONVERSATION_PAGE_SIZE).contains(&query.page_size()) {
            return Err(ListConversationsError::InvalidPageSize {
                value: query.page_size(),
                max: MAX_CONVERSATION_PAGE_SIZE,
            });
        }
        let fetch_limit = query
            .page_size()
            .checked_add(1)
            .and_then(|limit| i64::try_from(limit).ok())
            .ok_or(ListConversationsError::InvalidStoreResult)?;
        let before = query
            .before_conversation_id()
            .map(chat::ConversationId::get);
        let rows = sqlx::query_as::<_, ConversationWithRoleRow>(
            "SELECT c.id, c.title, c.created_at_ms, membership.role \
             FROM conversation_members AS membership \
             JOIN conversations AS c ON c.id = membership.conversation_id \
             WHERE membership.user_id = ? AND (? IS NULL OR c.id < ?) \
             ORDER BY c.id DESC \
             LIMIT ?",
        )
        .bind(actor_id.get())
        .bind(before)
        .bind(before)
        .bind(fetch_limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| ListConversationsError::StoreUnavailable)?;

        let mut conversations = rows
            .into_iter()
            .map(ConversationWithRoleRow::into_summary)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ListConversationsError::InvalidStoreResult)?;
        let has_more = conversations.len() > query.page_size();
        if has_more {
            conversations.truncate(query.page_size());
        }
        let next_cursor = if has_more {
            conversations.last().map(|entry| entry.conversation().id())
        } else {
            None
        };

        Ok(ConversationPage::new(conversations, next_cursor))
    }
}

impl ListMembersStore for SqliteStore {
    async fn list_members(
        &self,
        actor_id: UserId,
        query: ListMembers,
    ) -> Result<MemberPage, ListMembersError> {
        if !(1..=MAX_MEMBER_PAGE_SIZE).contains(&query.page_size()) {
            return Err(ListMembersError::InvalidPageSize {
                value: query.page_size(),
                max: MAX_MEMBER_PAGE_SIZE,
            });
        }
        let fetch_limit = query
            .page_size()
            .checked_add(1)
            .and_then(|limit| i64::try_from(limit).ok())
            .ok_or(ListMembersError::InvalidStoreResult)?;
        let after = query.after_user_id().map(chat::UserId::get);
        let rows = sqlx::query_as::<_, OptionalConversationMemberRow>(
            "SELECT viewer.role AS viewer_role, \
                    member.conversation_id, member.user_id, member.role, member.joined_at_ms, \
                    account.display_name, account.created_at_ms AS user_created_at_ms \
             FROM conversation_members AS viewer \
             LEFT JOIN conversation_members AS member \
               ON member.conversation_id = viewer.conversation_id \
              AND (? IS NULL OR member.user_id > ?) \
             LEFT JOIN users AS account ON account.id = member.user_id \
             WHERE viewer.conversation_id = ? AND viewer.user_id = ? \
             ORDER BY member.user_id ASC \
             LIMIT ?",
        )
        .bind(after)
        .bind(after)
        .bind(query.conversation_id().get())
        .bind(actor_id.get())
        .bind(fetch_limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| ListMembersError::StoreUnavailable)?;

        if rows.is_empty() {
            return Err(ListMembersError::NotFound);
        }

        let mut members = rows
            .into_iter()
            .map(OptionalConversationMemberRow::into_member)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ListMembersError::InvalidStoreResult)?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let has_more = members.len() > query.page_size();
        if has_more {
            members.truncate(query.page_size());
        }
        let next_cursor = if has_more {
            members.last().map(|member| member.user().id())
        } else {
            None
        };

        Ok(MemberPage::new(members, next_cursor))
    }
}

impl ListMessagesStore for SqliteStore {
    async fn list_messages(
        &self,
        actor_id: UserId,
        query: ListMessages,
    ) -> Result<MessagePage, ListMessagesError> {
        if !(1..=MAX_MESSAGE_PAGE_SIZE).contains(&query.page_size()) {
            return Err(ListMessagesError::InvalidPageSize {
                value: query.page_size(),
                max: MAX_MESSAGE_PAGE_SIZE,
            });
        }

        let fetch_limit = query
            .page_size()
            .checked_add(1)
            .and_then(|limit| i64::try_from(limit).ok())
            .ok_or(ListMessagesError::InvalidStoreResult)?;
        let before = query.before_message_id().map(chat::MessageId::get);

        let rows = sqlx::query_as::<_, OptionalMessageRow>(
            "SELECT viewer.role AS viewer_role, \
                    message.id, message.conversation_id, message.author_id, \
                    message.body, message.created_at_ms \
             FROM conversation_members AS viewer \
             LEFT JOIN messages AS message \
               ON message.conversation_id = viewer.conversation_id \
              AND (? IS NULL OR message.id < ?) \
             WHERE viewer.conversation_id = ? AND viewer.user_id = ? \
             ORDER BY message.id DESC \
             LIMIT ?",
        )
        .bind(before)
        .bind(before)
        .bind(query.conversation_id().get())
        .bind(actor_id.get())
        .bind(fetch_limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| ListMessagesError::StoreUnavailable)?;

        if rows.is_empty() {
            return Err(ListMessagesError::NotFound);
        }

        let mut messages = rows
            .into_iter()
            .map(OptionalMessageRow::into_message)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ListMessagesError::InvalidStoreResult)?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

        let has_more = messages.len() > query.page_size();
        if has_more {
            messages.truncate(query.page_size());
        }
        let next_cursor = if has_more {
            messages.last().map(chat::Message::id)
        } else {
            None
        };

        Ok(MessagePage::new(messages, next_cursor))
    }
}
