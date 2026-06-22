CREATE TABLE admission_codes (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    token_hash    BLOB NOT NULL UNIQUE CHECK (length(token_hash) = 32),
    created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
    expires_at_ms INTEGER NOT NULL CHECK (expires_at_ms > created_at_ms)
) STRICT;

CREATE INDEX admission_codes_by_expiry
    ON admission_codes (expires_at_ms);

ALTER TABLE oidc_login_transactions
    ADD COLUMN admission_code_id INTEGER
    REFERENCES admission_codes(id) ON DELETE SET NULL;
