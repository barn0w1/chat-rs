use chat::{
    AddMemberError, AddMemberStore, Conversation, ConversationId, CreateConversationError,
    CreateConversationStore, CreateUserError, CreateUserStore, CreatedConversation, Membership,
    MembershipRemoval, MembershipRole, NewConversation, NewMembership, NewMessage, NewUser,
    PostMessageError, PostMessageStore, RemoveMemberError, RemoveMemberStore, User,
};

use super::{
    SqliteStore, is_unique_violation, now_millis,
    row::{MEMBER_ROLE, MembershipRow, OWNER_ROLE, role_from_db},
};

impl CreateUserStore for SqliteStore {
    async fn create_user(&self, user: NewUser) -> Result<User, CreateUserError> {
        let created_at_ms = now_millis().map_err(|_| CreateUserError::StoreUnavailable)?;
        let id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users (display_name, created_at_ms) VALUES (?, ?) RETURNING id",
        )
        .bind(user.display_name().as_str())
        .bind(created_at_ms)
        .fetch_one(&self.pool)
        .await
        .map_err(|_| CreateUserError::StoreUnavailable)?;

        let id = chat::UserId::new(id).map_err(|_| CreateUserError::InvalidStoreResult)?;
        let created_at = super::system_time_from_millis(created_at_ms)
            .map_err(|_| CreateUserError::InvalidStoreResult)?;
        Ok(User::new(id, user.display_name().clone(), created_at))
    }
}

impl CreateConversationStore for SqliteStore {
    async fn create_conversation(
        &self,
        conversation: NewConversation,
    ) -> Result<CreatedConversation, CreateConversationError> {
        let created_at_ms = now_millis().map_err(|_| CreateConversationError::StoreUnavailable)?;
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|_| CreateConversationError::StoreUnavailable)?;

        if !user_exists(&mut transaction, conversation.creator_id().get())
            .await
            .map_err(|_| CreateConversationError::StoreUnavailable)?
        {
            return Err(CreateConversationError::CreatorNotFound);
        }

        let conversation_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO conversations (title, created_at_ms) VALUES (?, ?) RETURNING id",
        )
        .bind(conversation.title().as_str())
        .bind(created_at_ms)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|_| CreateConversationError::StoreUnavailable)?;

        sqlx::query(
            "INSERT INTO conversation_members \
             (conversation_id, user_id, role, joined_at_ms) VALUES (?, ?, ?, ?)",
        )
        .bind(conversation_id)
        .bind(conversation.creator_id().get())
        .bind(OWNER_ROLE)
        .bind(created_at_ms)
        .execute(&mut *transaction)
        .await
        .map_err(|_| CreateConversationError::StoreUnavailable)?;

        transaction
            .commit()
            .await
            .map_err(|_| CreateConversationError::StoreUnavailable)?;

        let conversation_id = ConversationId::new(conversation_id)
            .map_err(|_| CreateConversationError::InvalidStoreResult)?;
        let created_at = super::system_time_from_millis(created_at_ms)
            .map_err(|_| CreateConversationError::InvalidStoreResult)?;
        let stored_conversation =
            Conversation::new(conversation_id, conversation.title().clone(), created_at);
        let membership = Membership::new(
            conversation_id,
            conversation.creator_id(),
            MembershipRole::Owner,
            created_at,
        );
        Ok(CreatedConversation::new(stored_conversation, membership))
    }
}

impl AddMemberStore for SqliteStore {
    async fn add_member(&self, membership: NewMembership) -> Result<Membership, AddMemberError> {
        let joined_at_ms = now_millis().map_err(|_| AddMemberError::StoreUnavailable)?;
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|_| AddMemberError::StoreUnavailable)?;

        if !conversation_exists(&mut transaction, membership.conversation_id().get())
            .await
            .map_err(|_| AddMemberError::StoreUnavailable)?
        {
            return Err(AddMemberError::ConversationNotFound);
        }

        let actor_role = membership_role(
            &mut transaction,
            membership.conversation_id().get(),
            membership.actor_id().get(),
        )
        .await
        .map_err(|_| AddMemberError::StoreUnavailable)?;
        let Some(actor_role) = actor_role else {
            return Err(AddMemberError::NotOwner);
        };
        if role_from_db(&actor_role).map_err(|_| AddMemberError::InvalidStoreResult)?
            != MembershipRole::Owner
        {
            return Err(AddMemberError::NotOwner);
        }

        if !user_exists(&mut transaction, membership.user_id().get())
            .await
            .map_err(|_| AddMemberError::StoreUnavailable)?
        {
            return Err(AddMemberError::UserNotFound);
        }

        if let Some(existing_role) = membership_role(
            &mut transaction,
            membership.conversation_id().get(),
            membership.user_id().get(),
        )
        .await
        .map_err(|_| AddMemberError::StoreUnavailable)?
        {
            role_from_db(&existing_role).map_err(|_| AddMemberError::InvalidStoreResult)?;
            return Err(AddMemberError::AlreadyMember);
        }

        let result = sqlx::query(
            "INSERT INTO conversation_members \
             (conversation_id, user_id, role, joined_at_ms) VALUES (?, ?, ?, ?)",
        )
        .bind(membership.conversation_id().get())
        .bind(membership.user_id().get())
        .bind(MEMBER_ROLE)
        .bind(joined_at_ms)
        .execute(&mut *transaction)
        .await;
        if let Err(error) = result {
            return if is_unique_violation(&error) {
                Err(AddMemberError::AlreadyMember)
            } else {
                Err(AddMemberError::StoreUnavailable)
            };
        }

        transaction
            .commit()
            .await
            .map_err(|_| AddMemberError::StoreUnavailable)?;

        let joined_at = super::system_time_from_millis(joined_at_ms)
            .map_err(|_| AddMemberError::InvalidStoreResult)?;
        Ok(Membership::new(
            membership.conversation_id(),
            membership.user_id(),
            MembershipRole::Member,
            joined_at,
        ))
    }
}

impl RemoveMemberStore for SqliteStore {
    async fn remove_member(
        &self,
        removal: MembershipRemoval,
    ) -> Result<Membership, RemoveMemberError> {
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|_| RemoveMemberError::StoreUnavailable)?;

        if !conversation_exists(&mut transaction, removal.conversation_id().get())
            .await
            .map_err(|_| RemoveMemberError::StoreUnavailable)?
        {
            return Err(RemoveMemberError::ConversationNotFound);
        }

        let target = sqlx::query_as::<_, MembershipRow>(
            "SELECT conversation_id, user_id, role, joined_at_ms \
             FROM conversation_members WHERE conversation_id = ? AND user_id = ?",
        )
        .bind(removal.conversation_id().get())
        .bind(removal.user_id().get())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| RemoveMemberError::StoreUnavailable)?
        .ok_or(RemoveMemberError::NotMember)?
        .into_membership()
        .map_err(|_| RemoveMemberError::InvalidStoreResult)?;

        if removal.actor_id() == removal.user_id() {
            if target.role() == MembershipRole::Owner {
                return Err(RemoveMemberError::OwnerCannotLeave);
            }
        } else {
            let actor_role = membership_role(
                &mut transaction,
                removal.conversation_id().get(),
                removal.actor_id().get(),
            )
            .await
            .map_err(|_| RemoveMemberError::StoreUnavailable)?;
            let Some(actor_role) = actor_role else {
                return Err(RemoveMemberError::NotAuthorized);
            };
            if role_from_db(&actor_role).map_err(|_| RemoveMemberError::InvalidStoreResult)?
                != MembershipRole::Owner
            {
                return Err(RemoveMemberError::NotAuthorized);
            }
            if target.role() == MembershipRole::Owner {
                return Err(RemoveMemberError::OwnerCannotLeave);
            }
        }

        let result = sqlx::query(
            "DELETE FROM conversation_members WHERE conversation_id = ? AND user_id = ?",
        )
        .bind(removal.conversation_id().get())
        .bind(removal.user_id().get())
        .execute(&mut *transaction)
        .await
        .map_err(|_| RemoveMemberError::StoreUnavailable)?;
        if result.rows_affected() != 1 {
            return Err(RemoveMemberError::InvalidStoreResult);
        }

        transaction
            .commit()
            .await
            .map_err(|_| RemoveMemberError::StoreUnavailable)?;
        Ok(target)
    }
}

impl PostMessageStore for SqliteStore {
    async fn create_message(&self, message: NewMessage) -> Result<chat::Message, PostMessageError> {
        let created_at_ms = now_millis().map_err(|_| PostMessageError::StoreUnavailable)?;
        let mut transaction = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|_| PostMessageError::StoreUnavailable)?;

        if !conversation_exists(&mut transaction, message.conversation_id().get())
            .await
            .map_err(|_| PostMessageError::StoreUnavailable)?
        {
            return Err(PostMessageError::ConversationNotFound);
        }

        let author_role = membership_role(
            &mut transaction,
            message.conversation_id().get(),
            message.author_id().get(),
        )
        .await
        .map_err(|_| PostMessageError::StoreUnavailable)?;
        let Some(author_role) = author_role else {
            return Err(PostMessageError::AuthorNotMember);
        };
        role_from_db(&author_role).map_err(|_| PostMessageError::InvalidStoreResult)?;

        let id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO messages \
             (conversation_id, author_id, body, created_at_ms) VALUES (?, ?, ?, ?) RETURNING id",
        )
        .bind(message.conversation_id().get())
        .bind(message.author_id().get())
        .bind(message.body().as_str())
        .bind(created_at_ms)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|_| PostMessageError::StoreUnavailable)?;

        transaction
            .commit()
            .await
            .map_err(|_| PostMessageError::StoreUnavailable)?;

        let id = chat::MessageId::new(id).map_err(|_| PostMessageError::InvalidStoreResult)?;
        let created_at = super::system_time_from_millis(created_at_ms)
            .map_err(|_| PostMessageError::InvalidStoreResult)?;
        Ok(chat::Message::new(
            id,
            message.conversation_id(),
            message.author_id(),
            message.body().clone(),
            created_at,
        ))
    }
}

async fn user_exists(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    user_id: i64,
) -> Result<bool, sqlx::Error> {
    let exists = sqlx::query_scalar::<_, i64>("SELECT EXISTS(SELECT 1 FROM users WHERE id = ?)")
        .bind(user_id)
        .fetch_one(&mut **transaction)
        .await?;
    Ok(exists != 0)
}

async fn conversation_exists(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    conversation_id: i64,
) -> Result<bool, sqlx::Error> {
    let exists =
        sqlx::query_scalar::<_, i64>("SELECT EXISTS(SELECT 1 FROM conversations WHERE id = ?)")
            .bind(conversation_id)
            .fetch_one(&mut **transaction)
            .await?;
    Ok(exists != 0)
}

async fn membership_role(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    conversation_id: i64,
    user_id: i64,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>(
        "SELECT role FROM conversation_members WHERE conversation_id = ? AND user_id = ?",
    )
    .bind(conversation_id)
    .bind(user_id)
    .fetch_optional(&mut **transaction)
    .await
}
