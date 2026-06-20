use chat::{
    Conversation, ConversationDetails, ConversationId, ConversationMember, ConversationSummary,
    ConversationTitle, DisplayName, Membership, MembershipRole, Message, MessageBody, MessageId,
    User, UserId,
};
use sqlx::FromRow;

use super::{InvalidStoredData, system_time_from_millis};

pub(super) const OWNER_ROLE: &str = "owner";
pub(super) const MEMBER_ROLE: &str = "member";

#[derive(Debug, FromRow)]
pub(super) struct ConversationRow {
    pub(super) id: i64,
    pub(super) title: String,
    pub(super) created_at_ms: i64,
}

impl ConversationRow {
    pub(super) fn into_conversation(self) -> Result<Conversation, InvalidStoredData> {
        Ok(Conversation::new(
            ConversationId::new(self.id).map_err(|_| InvalidStoredData)?,
            ConversationTitle::try_from(self.title).map_err(|_| InvalidStoredData)?,
            system_time_from_millis(self.created_at_ms)?,
        ))
    }
}

#[derive(Debug, FromRow)]
pub(super) struct ConversationWithRoleRow {
    pub(super) id: i64,
    pub(super) title: String,
    pub(super) created_at_ms: i64,
    pub(super) role: String,
}

impl ConversationWithRoleRow {
    pub(super) fn into_details(self) -> Result<ConversationDetails, InvalidStoredData> {
        let role = role_from_db(&self.role)?;
        let conversation = ConversationRow {
            id: self.id,
            title: self.title,
            created_at_ms: self.created_at_ms,
        }
        .into_conversation()?;
        Ok(ConversationDetails::new(conversation, role))
    }

    pub(super) fn into_summary(self) -> Result<ConversationSummary, InvalidStoredData> {
        let role = role_from_db(&self.role)?;
        let conversation = ConversationRow {
            id: self.id,
            title: self.title,
            created_at_ms: self.created_at_ms,
        }
        .into_conversation()?;
        Ok(ConversationSummary::new(conversation, role))
    }
}

#[derive(Debug, FromRow)]
pub(super) struct MembershipRow {
    pub(super) conversation_id: i64,
    pub(super) user_id: i64,
    pub(super) role: String,
    pub(super) joined_at_ms: i64,
}

impl MembershipRow {
    pub(super) fn into_membership(self) -> Result<Membership, InvalidStoredData> {
        Ok(Membership::new(
            ConversationId::new(self.conversation_id).map_err(|_| InvalidStoredData)?,
            UserId::new(self.user_id).map_err(|_| InvalidStoredData)?,
            role_from_db(&self.role)?,
            system_time_from_millis(self.joined_at_ms)?,
        ))
    }
}

#[derive(Debug, FromRow)]
pub(super) struct ConversationMemberRow {
    pub(super) conversation_id: i64,
    pub(super) user_id: i64,
    pub(super) role: String,
    pub(super) joined_at_ms: i64,
    pub(super) display_name: String,
    pub(super) user_created_at_ms: i64,
}

impl ConversationMemberRow {
    pub(super) fn into_member(self) -> Result<ConversationMember, InvalidStoredData> {
        let user_id = UserId::new(self.user_id).map_err(|_| InvalidStoredData)?;
        let user = User::new(
            user_id,
            DisplayName::try_from(self.display_name).map_err(|_| InvalidStoredData)?,
            system_time_from_millis(self.user_created_at_ms)?,
        );
        let membership = Membership::new(
            ConversationId::new(self.conversation_id).map_err(|_| InvalidStoredData)?,
            user_id,
            role_from_db(&self.role)?,
            system_time_from_millis(self.joined_at_ms)?,
        );
        ConversationMember::new(user, membership).map_err(|_| InvalidStoredData)
    }
}

#[derive(Debug, FromRow)]
pub(super) struct MessageRow {
    pub(super) id: i64,
    pub(super) conversation_id: i64,
    pub(super) author_id: i64,
    pub(super) body: String,
    pub(super) created_at_ms: i64,
}

impl MessageRow {
    pub(super) fn into_message(self) -> Result<Message, InvalidStoredData> {
        Ok(Message::new(
            MessageId::new(self.id).map_err(|_| InvalidStoredData)?,
            ConversationId::new(self.conversation_id).map_err(|_| InvalidStoredData)?,
            UserId::new(self.author_id).map_err(|_| InvalidStoredData)?,
            MessageBody::try_from(self.body).map_err(|_| InvalidStoredData)?,
            system_time_from_millis(self.created_at_ms)?,
        ))
    }
}

#[derive(Debug, FromRow)]
pub(super) struct OptionalMessageRow {
    pub(super) viewer_role: String,
    pub(super) id: Option<i64>,
    pub(super) conversation_id: Option<i64>,
    pub(super) author_id: Option<i64>,
    pub(super) body: Option<String>,
    pub(super) created_at_ms: Option<i64>,
}

impl OptionalMessageRow {
    pub(super) fn into_message(self) -> Result<Option<Message>, InvalidStoredData> {
        role_from_db(&self.viewer_role)?;
        match (
            self.id,
            self.conversation_id,
            self.author_id,
            self.body,
            self.created_at_ms,
        ) {
            (None, None, None, None, None) => Ok(None),
            (Some(id), Some(conversation_id), Some(author_id), Some(body), Some(created_at_ms)) => {
                MessageRow {
                    id,
                    conversation_id,
                    author_id,
                    body,
                    created_at_ms,
                }
                .into_message()
                .map(Some)
            }
            _ => Err(InvalidStoredData),
        }
    }
}

pub(super) fn role_from_db(value: &str) -> Result<MembershipRole, InvalidStoredData> {
    match value {
        OWNER_ROLE => Ok(MembershipRole::Owner),
        MEMBER_ROLE => Ok(MembershipRole::Member),
        _ => Err(InvalidStoredData),
    }
}
