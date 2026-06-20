use chat::{
    ConversationDetails, ConversationMember, ConversationSummary, GetConversationError,
    GetConversationStore, ListConversationsError, ListConversationsStore, ListMembersError,
    ListMembersStore, ListMessages, ListMessagesError, ListMessagesStore, MAX_MESSAGE_PAGE_SIZE,
    MessagePage, UserId,
};

use super::{
    SqliteStore,
    row::{ConversationMemberRow, ConversationWithRoleRow, OptionalMessageRow},
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
    ) -> Result<Vec<ConversationSummary>, ListConversationsError> {
        let rows = sqlx::query_as::<_, ConversationWithRoleRow>(
            "SELECT c.id, c.title, c.created_at_ms, membership.role \
             FROM conversation_members AS membership \
             JOIN conversations AS c ON c.id = membership.conversation_id \
             WHERE membership.user_id = ? \
             ORDER BY c.id DESC",
        )
        .bind(actor_id.get())
        .fetch_all(&self.pool)
        .await
        .map_err(|_| ListConversationsError::StoreUnavailable)?;

        rows.into_iter()
            .map(ConversationWithRoleRow::into_summary)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ListConversationsError::InvalidStoreResult)
    }
}

impl ListMembersStore for SqliteStore {
    async fn list_members(
        &self,
        actor_id: UserId,
        conversation_id: chat::ConversationId,
    ) -> Result<Vec<ConversationMember>, ListMembersError> {
        let rows = sqlx::query_as::<_, ConversationMemberRow>(
            "SELECT member.conversation_id, member.user_id, member.role, member.joined_at_ms, \
                    account.display_name, account.created_at_ms AS user_created_at_ms \
             FROM conversation_members AS viewer \
             JOIN conversation_members AS member \
               ON member.conversation_id = viewer.conversation_id \
             JOIN users AS account ON account.id = member.user_id \
             WHERE viewer.conversation_id = ? AND viewer.user_id = ? \
             ORDER BY CASE member.role WHEN 'owner' THEN 0 ELSE 1 END, \
                      member.joined_at_ms, member.user_id",
        )
        .bind(conversation_id.get())
        .bind(actor_id.get())
        .fetch_all(&self.pool)
        .await
        .map_err(|_| ListMembersError::StoreUnavailable)?;

        if rows.is_empty() {
            return Err(ListMembersError::NotFound);
        }

        rows.into_iter()
            .map(ConversationMemberRow::into_member)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ListMembersError::InvalidStoreResult)
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
        let next_cursor = has_more.then(|| {
            messages
                .last()
                .expect("a page with another row contains a returned message")
                .id()
        });

        Ok(MessagePage::new(messages, next_cursor))
    }
}
