use std::time::{Duration, SystemTime};

use chat::{DisplayName, User, UserId};
use sqlx::FromRow;

use crate::sqlite::{SqliteStore, is_unique_violation, system_time_from_millis};

use super::{
    AuthError, AuthenticatedSession, ConsumedOidcLogin, IssuedSession, OidcLoginTransaction,
    SecretToken, VerifiedIdentity, unix_time_millis,
};

const SESSION_LIFETIME: Duration = Duration::from_secs(30 * 24 * 60 * 60);
const LOGIN_LIFETIME: Duration = Duration::from_secs(10 * 60);
const TOKEN_INSERT_ATTEMPTS: usize = 2;

#[derive(Clone, Debug)]
pub(crate) struct AuthStore {
    sqlite: SqliteStore,
}

impl AuthStore {
    pub(crate) const fn new(sqlite: SqliteStore) -> Self {
        Self { sqlite }
    }

    pub(crate) async fn resolve_or_provision(
        &self,
        identity: &VerifiedIdentity,
        now: SystemTime,
    ) -> Result<User, AuthError> {
        let created_at_ms = unix_time_millis(now)?;
        let mut transaction = self.sqlite.pool.begin_with("BEGIN IMMEDIATE").await?;

        if let Some(row) = sqlx::query_as::<_, UserRow>(
            "SELECT u.id, u.display_name, u.created_at_ms \
             FROM auth_identities AS i \
             JOIN users AS u ON u.id = i.user_id \
             WHERE i.authority = ? AND i.subject = ?",
        )
        .bind(identity.authority())
        .bind(identity.subject())
        .fetch_optional(&mut *transaction)
        .await?
        {
            let user = row.into_user()?;
            transaction.commit().await?;
            return Ok(user);
        }

        let user_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users (display_name, created_at_ms) VALUES (?, ?) RETURNING id",
        )
        .bind(identity.display_name().as_str())
        .bind(created_at_ms)
        .fetch_one(&mut *transaction)
        .await?;

        sqlx::query(
            "INSERT INTO auth_identities \
             (authority, subject, user_id, created_at_ms) VALUES (?, ?, ?, ?)",
        )
        .bind(identity.authority())
        .bind(identity.subject())
        .bind(user_id)
        .bind(created_at_ms)
        .execute(&mut *transaction)
        .await?;

        transaction.commit().await?;
        UserRow {
            id: user_id,
            display_name: identity.display_name().as_str().to_owned(),
            created_at_ms,
        }
        .into_user()
    }

    pub(crate) async fn issue_session(
        &self,
        user_id: UserId,
        previous_token: Option<&str>,
        now: SystemTime,
    ) -> Result<IssuedSession, AuthError> {
        let created_at_ms = unix_time_millis(now)?;
        let expires_at_ms = expiry_millis(now, SESSION_LIFETIME)?;
        let previous_hash = previous_token
            .and_then(|value| SecretToken::parse(value).ok())
            .map(|token| token.hash());

        for _ in 0..TOKEN_INSERT_ATTEMPTS {
            let session_token = SecretToken::generate()?;
            let csrf_token = SecretToken::generate()?;
            let mut transaction = self.sqlite.pool.begin_with("BEGIN IMMEDIATE").await?;

            sqlx::query("DELETE FROM auth_sessions WHERE expires_at_ms <= ?")
                .bind(created_at_ms)
                .execute(&mut *transaction)
                .await?;
            if let Some(previous_hash) = previous_hash {
                sqlx::query("DELETE FROM auth_sessions WHERE token_hash = ?")
                    .bind(previous_hash.as_slice())
                    .execute(&mut *transaction)
                    .await?;
            }

            let session_hash = session_token.hash();
            let result = sqlx::query(
                "INSERT INTO auth_sessions \
                 (token_hash, csrf_token, user_id, created_at_ms, expires_at_ms) \
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(session_hash.as_slice())
            .bind(csrf_token.bytes().as_slice())
            .bind(user_id.get())
            .bind(created_at_ms)
            .bind(expires_at_ms)
            .execute(&mut *transaction)
            .await;

            match result {
                Ok(_) => {
                    transaction.commit().await?;
                    return Ok(IssuedSession::new(session_token));
                }
                Err(error) if is_unique_violation(&error) => continue,
                Err(error) => return Err(error.into()),
            }
        }

        Err(AuthError::InvalidStoredData)
    }

    pub(crate) async fn resolve_session(
        &self,
        token: &str,
        now: SystemTime,
    ) -> Result<Option<AuthenticatedSession>, AuthError> {
        let token = match SecretToken::parse(token) {
            Ok(token) => token,
            Err(_) => return Ok(None),
        };
        let now_ms = unix_time_millis(now)?;
        let row = sqlx::query_as::<_, SessionUserRow>(
            "SELECT u.id, u.display_name, u.created_at_ms, s.csrf_token \
             FROM auth_sessions AS s \
             JOIN users AS u ON u.id = s.user_id \
             WHERE s.token_hash = ? AND s.expires_at_ms > ?",
        )
        .bind(token.hash().as_slice())
        .bind(now_ms)
        .fetch_optional(&self.sqlite.pool)
        .await?;

        row.map(SessionUserRow::into_session).transpose()
    }

    pub(crate) async fn revoke_session(&self, token: &str) -> Result<(), AuthError> {
        let Ok(token) = SecretToken::parse(token) else {
            return Ok(());
        };
        sqlx::query("DELETE FROM auth_sessions WHERE token_hash = ?")
            .bind(token.hash().as_slice())
            .execute(&self.sqlite.pool)
            .await?;
        Ok(())
    }

    pub(crate) async fn store_oidc_login(
        &self,
        login: &OidcLoginTransaction,
        now: SystemTime,
    ) -> Result<(), AuthError> {
        let created_at_ms = unix_time_millis(now)?;
        let expires_at_ms = expiry_millis(now, LOGIN_LIFETIME)?;
        let mut transaction = self.sqlite.pool.begin_with("BEGIN IMMEDIATE").await?;

        sqlx::query("DELETE FROM oidc_login_transactions WHERE expires_at_ms <= ?")
            .bind(created_at_ms)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            "INSERT INTO oidc_login_transactions \
             (state_hash, browser_binding_hash, nonce, pkce_verifier, \
              created_at_ms, expires_at_ms) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(login.state().hash().as_slice())
        .bind(login.browser_binding().hash().as_slice())
        .bind(login.nonce())
        .bind(login.pkce_verifier())
        .bind(created_at_ms)
        .bind(expires_at_ms)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub(crate) async fn consume_oidc_login(
        &self,
        state: &str,
        browser_binding: &str,
        now: SystemTime,
    ) -> Result<ConsumedOidcLogin, AuthError> {
        let state = SecretToken::parse(state).map_err(|_| AuthError::LoginTransactionRejected)?;
        let browser_binding =
            SecretToken::parse(browser_binding).map_err(|_| AuthError::LoginTransactionRejected)?;
        let now_ms = unix_time_millis(now)?;
        let state_hash = state.hash();
        let mut transaction = self.sqlite.pool.begin_with("BEGIN IMMEDIATE").await?;

        let row = sqlx::query_as::<_, LoginRow>(
            "SELECT browser_binding_hash, nonce, pkce_verifier \
             FROM oidc_login_transactions \
             WHERE state_hash = ? AND expires_at_ms > ?",
        )
        .bind(state_hash.as_slice())
        .bind(now_ms)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(AuthError::LoginTransactionRejected)?;

        let expected_binding = fixed_hash(row.browser_binding_hash)?;
        if !browser_binding.matches_hash(&expected_binding) {
            return Err(AuthError::LoginTransactionRejected);
        }

        let deleted = sqlx::query("DELETE FROM oidc_login_transactions WHERE state_hash = ?")
            .bind(state_hash.as_slice())
            .execute(&mut *transaction)
            .await?;
        if deleted.rows_affected() != 1 {
            return Err(AuthError::LoginTransactionRejected);
        }

        transaction.commit().await?;
        Ok(ConsumedOidcLogin::new(row.nonce, row.pkce_verifier))
    }
}

fn expiry_millis(now: SystemTime, lifetime: Duration) -> Result<i64, AuthError> {
    now.checked_add(lifetime)
        .ok_or(AuthError::TimeUnavailable)
        .and_then(unix_time_millis)
}

fn fixed_hash(value: Vec<u8>) -> Result<[u8; 32], AuthError> {
    value.try_into().map_err(|_| AuthError::InvalidStoredData)
}

#[derive(Debug, FromRow)]
struct UserRow {
    id: i64,
    display_name: String,
    created_at_ms: i64,
}

impl UserRow {
    fn into_user(self) -> Result<User, AuthError> {
        Ok(User::new(
            UserId::new(self.id).map_err(|_| AuthError::InvalidStoredData)?,
            DisplayName::try_from(self.display_name).map_err(|_| AuthError::InvalidStoredData)?,
            system_time_from_millis(self.created_at_ms)
                .map_err(|_| AuthError::InvalidStoredData)?,
        ))
    }
}

#[derive(Debug, FromRow)]
struct SessionUserRow {
    id: i64,
    display_name: String,
    created_at_ms: i64,
    csrf_token: Vec<u8>,
}

impl SessionUserRow {
    fn into_session(self) -> Result<AuthenticatedSession, AuthError> {
        let csrf_token = SecretToken::from_stored(self.csrf_token)?;
        let user = UserRow {
            id: self.id,
            display_name: self.display_name,
            created_at_ms: self.created_at_ms,
        }
        .into_user()?;
        Ok(AuthenticatedSession::new(user, csrf_token))
    }
}

#[derive(Debug, FromRow)]
struct LoginRow {
    browser_binding_hash: Vec<u8>,
    nonce: String,
    pkce_verifier: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chat::{Chat, CreateUser};
    use tempfile::TempDir;

    async fn test_store() -> (AuthStore, SqliteStore, TempDir) {
        let directory = tempfile::tempdir().expect("temporary directory can be created");
        let sqlite = SqliteStore::open(directory.path().join("chat.sqlite3"))
            .await
            .expect("database can be opened");
        (AuthStore::new(sqlite.clone()), sqlite, directory)
    }

    #[tokio::test]
    async fn identities_are_stable_and_names_do_not_follow_provider_changes() {
        let (store, sqlite, _directory) = test_store().await;
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
        let first =
            VerifiedIdentity::new("issuer", "subject", Some("Alice")).expect("identity is valid");
        let user = store
            .resolve_or_provision(&first, now)
            .await
            .expect("user can be provisioned");

        let changed =
            VerifiedIdentity::new("issuer", "subject", Some("Changed")).expect("identity is valid");
        let resolved = store
            .resolve_or_provision(&changed, now)
            .await
            .expect("identity can be resolved");

        assert_eq!(resolved.id(), user.id());
        assert_eq!(resolved.display_name().as_str(), "Alice");
        sqlite.close().await;
    }

    #[tokio::test]
    async fn competing_identity_provisioning_creates_one_user() {
        let (store, sqlite, _directory) = test_store().await;
        let identity = VerifiedIdentity::new("issuer", "same-subject", Some("Alice"))
            .expect("identity is valid");
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(15);

        let (first, second) = tokio::join!(
            store.resolve_or_provision(&identity, now),
            store.resolve_or_provision(&identity, now),
        );

        assert_eq!(first.unwrap().id(), second.unwrap().id());
        let users = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
            .fetch_one(&sqlite.pool)
            .await
            .unwrap();
        assert_eq!(users, 1);
        sqlite.close().await;
    }

    #[tokio::test]
    async fn sessions_resolve_validate_csrf_and_revoke() {
        let (store, sqlite, _directory) = test_store().await;
        let chat = Chat::new(sqlite.clone());
        let user = chat
            .create_user(CreateUser::new("Alice"))
            .await
            .expect("user can be created")
            .user()
            .id();
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(20);
        let issued = store
            .issue_session(user, None, now)
            .await
            .expect("session can be issued");
        let token = issued.session_token();
        let resolved = store
            .resolve_session(&token, now)
            .await
            .expect("session lookup succeeds")
            .expect("session is valid");
        let csrf = resolved.csrf_token();
        assert_eq!(resolved.user_id(), user);
        assert!(resolved.verifies_csrf(&csrf));
        assert!(!resolved.verifies_csrf(&SecretToken::generate().unwrap().encode()));

        let expired = now + SESSION_LIFETIME + Duration::from_secs(1);
        assert!(
            store
                .resolve_session(&token, expired)
                .await
                .unwrap()
                .is_none()
        );

        store
            .revoke_session(&token)
            .await
            .expect("session can be revoked");
        assert!(store.resolve_session(&token, now).await.unwrap().is_none());
        sqlite.close().await;
    }

    #[tokio::test]
    async fn oidc_transactions_are_bound_single_use_and_expiring() {
        let (store, sqlite, _directory) = test_store().await;
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(30);
        let login = OidcLoginTransaction::new(
            SecretToken::generate().expect("state can be generated"),
            SecretToken::generate().expect("binding can be generated"),
            String::from("nonce"),
            "v".repeat(43),
        );
        store
            .store_oidc_login(&login, now)
            .await
            .expect("login can be stored");

        let consumed = store
            .consume_oidc_login(
                &login.state().encode(),
                &login.browser_binding().encode(),
                now,
            )
            .await
            .expect("matching login can be consumed");
        assert_eq!(consumed.nonce(), "nonce");
        assert!(
            store
                .consume_oidc_login(
                    &login.state().encode(),
                    &login.browser_binding().encode(),
                    now,
                )
                .await
                .is_err()
        );
        sqlite.close().await;
    }

    #[tokio::test]
    async fn oidc_transactions_reject_wrong_browser_and_expiry() {
        let (store, sqlite, _directory) = test_store().await;
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(30);
        let login = OidcLoginTransaction::new(
            SecretToken::generate().unwrap(),
            SecretToken::generate().unwrap(),
            String::from("nonce"),
            "v".repeat(43),
        );
        store.store_oidc_login(&login, now).await.unwrap();

        let wrong_binding = SecretToken::generate().unwrap().encode();
        assert!(
            store
                .consume_oidc_login(&login.state().encode(), &wrong_binding, now)
                .await
                .is_err()
        );
        assert!(
            store
                .consume_oidc_login(
                    &login.state().encode(),
                    &login.browser_binding().encode(),
                    now + LOGIN_LIFETIME + Duration::from_secs(1),
                )
                .await
                .is_err()
        );
        sqlite.close().await;
    }
}
