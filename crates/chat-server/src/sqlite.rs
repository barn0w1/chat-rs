//! SQLite persistence for the chat application.

mod read;
mod row;
mod write;

use std::{
    fmt,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use sqlx::{
    SqlitePool,
    migrate::{MigrateError, Migrator},
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

const MAX_CONNECTIONS: u32 = 4;
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// A SQLite-backed implementation of the storage capabilities required by `chat`.
#[derive(Clone, Debug)]
pub struct SqliteStore {
    pub(crate) pool: SqlitePool,
}

impl SqliteStore {
    /// Opens or creates a database, applies migrations, and returns a ready store.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, OpenError> {
        let options = connect_options(path.as_ref());
        let pool = SqlitePoolOptions::new()
            .max_connections(MAX_CONNECTIONS)
            .connect_with(options)
            .await
            .map_err(OpenError::Database)?;

        if let Err(error) = MIGRATOR.run(&pool).await {
            pool.close().await;
            return Err(OpenError::Migration(error));
        }

        Ok(Self { pool })
    }

    /// Closes the pool and waits for all connections to finish closing.
    pub async fn close(&self) {
        self.pool.close().await;
    }

    pub(crate) async fn check_readiness(&self) -> Result<(), sqlx::Error> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }
}

/// An error encountered while opening and preparing a SQLite store.
#[derive(Debug)]
pub enum OpenError {
    /// The database or connection pool could not be opened.
    Database(sqlx::Error),
    /// Embedded migrations could not be applied.
    Migration(MigrateError),
}

impl fmt::Display for OpenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "failed to open SQLite database: {error}"),
            Self::Migration(error) => {
                write!(formatter, "failed to migrate SQLite database: {error}")
            }
        }
    }
}

impl std::error::Error for OpenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::Migration(error) => Some(error),
        }
    }
}

fn connect_options(path: &Path) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Full)
        .busy_timeout(BUSY_TIMEOUT)
}

fn now_millis() -> Result<i64, InvalidStoredData> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| InvalidStoredData)?;
    i64::try_from(duration.as_millis()).map_err(|_| InvalidStoredData)
}

pub(crate) fn system_time_from_millis(value: i64) -> Result<SystemTime, InvalidStoredData> {
    let millis = u64::try_from(value).map_err(|_| InvalidStoredData)?;
    UNIX_EPOCH
        .checked_add(Duration::from_millis(millis))
        .ok_or(InvalidStoredData)
}

pub(crate) fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .is_some_and(|database_error| database_error.is_unique_violation())
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct InvalidStoredData;

#[cfg(test)]
mod tests {
    use super::*;
    use chat::{
        Chat, CreateConversation, CreateConversationError, CreateUser, ListMembers,
        ListMembersError,
    };

    #[test]
    fn timestamp_conversion_rejects_negative_values() {
        assert!(system_time_from_millis(-1).is_err());
    }

    #[tokio::test]
    async fn configured_connections_enforce_required_pragmas() {
        let directory = tempfile::tempdir().expect("temporary directory can be created");
        let store = SqliteStore::open(directory.path().join("chat.sqlite3"))
            .await
            .expect("database can be opened");

        let foreign_keys = sqlx::query_scalar::<_, i64>("PRAGMA foreign_keys")
            .fetch_one(&store.pool)
            .await
            .expect("foreign key mode can be read");
        let journal_mode = sqlx::query_scalar::<_, String>("PRAGMA journal_mode")
            .fetch_one(&store.pool)
            .await
            .expect("journal mode can be read");
        let synchronous = sqlx::query_scalar::<_, i64>("PRAGMA synchronous")
            .fetch_one(&store.pool)
            .await
            .expect("synchronous mode can be read");

        assert_eq!(foreign_keys, 1);
        assert_eq!(journal_mode, "wal");
        assert_eq!(synchronous, 2);
        store.close().await;
    }

    #[tokio::test]
    async fn failed_multi_record_mutation_rolls_back() {
        let directory = tempfile::tempdir().expect("temporary directory can be created");
        let store = SqliteStore::open(directory.path().join("chat.sqlite3"))
            .await
            .expect("database can be opened");
        let chat = Chat::new(store.clone());
        let owner = chat
            .create_user(CreateUser::new("Owner"))
            .await
            .expect("owner can be created")
            .user()
            .id();

        sqlx::query(
            "CREATE TRIGGER reject_owner_membership \
             BEFORE INSERT ON conversation_members \
             WHEN NEW.role = 'owner' \
             BEGIN SELECT RAISE(ABORT, 'test failure'); END",
        )
        .execute(&store.pool)
        .await
        .expect("failure trigger can be installed");

        assert_eq!(
            chat.create_conversation(owner, CreateConversation::new("Rolled back"))
                .await,
            Err(CreateConversationError::StoreUnavailable)
        );
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM conversations")
            .fetch_one(&store.pool)
            .await
            .expect("conversation count can be read");
        assert_eq!(count, 0);
        store.close().await;
    }

    #[tokio::test]
    async fn invalid_persisted_values_are_rejected() {
        let directory = tempfile::tempdir().expect("temporary directory can be created");
        let store = SqliteStore::open(directory.path().join("chat.sqlite3"))
            .await
            .expect("database can be opened");
        let chat = Chat::new(store.clone());
        let owner = chat
            .create_user(CreateUser::new("Owner"))
            .await
            .expect("owner can be created")
            .user()
            .id();
        let conversation = chat
            .create_conversation(owner, CreateConversation::new("General"))
            .await
            .expect("conversation can be created")
            .conversation()
            .id();

        sqlx::query("UPDATE users SET display_name = '   ' WHERE id = ?")
            .bind(owner.get())
            .execute(&store.pool)
            .await
            .expect("fixture corruption can be introduced");

        assert_eq!(
            chat.list_members(owner, ListMembers::new(conversation))
                .await,
            Err(ListMembersError::InvalidStoreResult)
        );
        store.close().await;
    }
}
