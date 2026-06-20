CREATE TABLE users (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    display_name  TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0)
) STRICT;

CREATE TABLE conversations (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    title         TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0)
) STRICT;

CREATE TABLE conversation_members (
    conversation_id INTEGER NOT NULL REFERENCES conversations(id),
    user_id          INTEGER NOT NULL REFERENCES users(id),
    role             TEXT NOT NULL CHECK (role IN ('owner', 'member')),
    joined_at_ms     INTEGER NOT NULL CHECK (joined_at_ms >= 0),
    PRIMARY KEY (conversation_id, user_id)
) STRICT;

CREATE TABLE messages (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id INTEGER NOT NULL REFERENCES conversations(id),
    author_id       INTEGER NOT NULL REFERENCES users(id),
    body            TEXT NOT NULL,
    created_at_ms   INTEGER NOT NULL CHECK (created_at_ms >= 0)
) STRICT;

CREATE UNIQUE INDEX conversation_members_one_owner
    ON conversation_members (conversation_id)
    WHERE role = 'owner';

CREATE INDEX conversation_members_by_user
    ON conversation_members (user_id, conversation_id);

CREATE INDEX messages_by_conversation
    ON messages (conversation_id, id DESC);
