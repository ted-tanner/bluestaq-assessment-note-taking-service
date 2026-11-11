CREATE TABLE users (
    id TEXT PRIMARY KEY NOT NULL,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    token_version BIGINT NOT NULL DEFAULT 0 CHECK (token_version >= 0),
    verified BOOLEAN NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL
);
CREATE INDEX users_pending ON users(verified, created_at);
CREATE TABLE email_challenges (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    purpose TEXT NOT NULL CHECK (purpose IN ('registration', 'signin', 'reset')),
    code_hash TEXT NOT NULL,
    expires_at BIGINT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    last_sent_at BIGINT NOT NULL,
    created_at BIGINT NOT NULL,
    UNIQUE(user_id, purpose)
);
CREATE INDEX challenges_expiry ON email_challenges(expires_at);
CREATE TABLE notes (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL
);
CREATE INDEX notes_owner_updated ON notes(user_id, updated_at DESC, id);

CREATE TABLE token_blacklist (
    token_hash TEXT PRIMARY KEY NOT NULL,
    expires_at BIGINT NOT NULL
);
CREATE INDEX token_blacklist_expiry ON token_blacklist(expires_at);
