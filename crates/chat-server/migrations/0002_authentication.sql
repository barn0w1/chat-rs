CREATE TABLE auth_identities (
    authority     TEXT NOT NULL CHECK (length(authority) BETWEEN 1 AND 2048),
    subject       TEXT NOT NULL CHECK (length(subject) BETWEEN 1 AND 1024),
    user_id       INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    PRIMARY KEY (authority, subject)
) STRICT;

CREATE INDEX auth_identities_by_user
    ON auth_identities (user_id);

CREATE TABLE auth_sessions (
    token_hash      BLOB PRIMARY KEY CHECK (length(token_hash) = 32),
    csrf_token      BLOB NOT NULL CHECK (length(csrf_token) = 32),
    user_id         INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at_ms   INTEGER NOT NULL CHECK (created_at_ms >= 0),
    expires_at_ms   INTEGER NOT NULL CHECK (expires_at_ms > created_at_ms)
) STRICT;

CREATE INDEX auth_sessions_by_user
    ON auth_sessions (user_id);

CREATE INDEX auth_sessions_by_expiry
    ON auth_sessions (expires_at_ms);

CREATE TABLE oidc_login_transactions (
    state_hash           BLOB PRIMARY KEY CHECK (length(state_hash) = 32),
    browser_binding_hash BLOB NOT NULL CHECK (length(browser_binding_hash) = 32),
    nonce                TEXT NOT NULL CHECK (length(nonce) BETWEEN 1 AND 1024),
    pkce_verifier        TEXT NOT NULL CHECK (length(pkce_verifier) BETWEEN 43 AND 128),
    created_at_ms        INTEGER NOT NULL CHECK (created_at_ms >= 0),
    expires_at_ms        INTEGER NOT NULL CHECK (expires_at_ms > created_at_ms)
) STRICT;

CREATE INDEX oidc_login_transactions_by_expiry
    ON oidc_login_transactions (expires_at_ms);
