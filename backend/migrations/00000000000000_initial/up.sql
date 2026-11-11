CREATE TABLE users (
    id TEXT PRIMARY KEY NOT NULL,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    token_version BIGINT NOT NULL DEFAULT 0 CHECK (token_version >= 0),
    verified BOOLEAN NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    name TEXT NOT NULL DEFAULT '',
    name_key TEXT NOT NULL DEFAULT ''
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
CREATE TABLE folders (
 id TEXT PRIMARY KEY NOT NULL,
 user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 name TEXT NOT NULL CHECK(length(trim(name)) BETWEEN 1 AND 255),
 parent_id TEXT REFERENCES folders(id),
 created_at BIGINT NOT NULL,
 updated_at BIGINT NOT NULL,
 revision BIGINT NOT NULL DEFAULT 1 CHECK(revision > 0)
);
CREATE INDEX folders_owner_parent ON folders(user_id, parent_id);

CREATE TABLE notes (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    revision BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0),
    folder_id TEXT REFERENCES folders(id)
);
CREATE INDEX notes_owner_updated ON notes(user_id, updated_at DESC, id);
CREATE INDEX notes_owner_folder_updated ON notes(user_id, folder_id, updated_at DESC, id);

CREATE TABLE token_blacklist (
    token_hash TEXT PRIMARY KEY NOT NULL,
    expires_at BIGINT NOT NULL
);
CREATE INDEX token_blacklist_expiry ON token_blacklist(expires_at);

CREATE VIRTUAL TABLE notes_fts USING fts5(title, body, content='notes', content_rowid='rowid', tokenize='unicode61 remove_diacritics 2', prefix='2 3 4');
CREATE TRIGGER notes_search_insert AFTER INSERT ON notes BEGIN
  INSERT INTO notes_fts(rowid, title, body) VALUES (new.rowid, new.title, new.body);
END;
CREATE TRIGGER notes_search_delete AFTER DELETE ON notes BEGIN
  INSERT INTO notes_fts(notes_fts, rowid, title, body) VALUES ('delete', old.rowid, old.title, old.body);
END;
CREATE TRIGGER notes_search_update AFTER UPDATE ON notes BEGIN
  INSERT INTO notes_fts(notes_fts, rowid, title, body) VALUES ('delete', old.rowid, old.title, old.body);
  INSERT INTO notes_fts(rowid, title, body) VALUES (new.rowid, new.title, new.body);
END;

CREATE TABLE note_shares (
 note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
 user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 permission TEXT NOT NULL CHECK(permission IN ('view','edit')),
 PRIMARY KEY(note_id,user_id)
);
CREATE INDEX note_shares_recipient ON note_shares(user_id,note_id);
CREATE TABLE folder_shares (
 folder_id TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
 user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 permission TEXT NOT NULL CHECK(permission IN ('view','edit')),
 PRIMARY KEY(folder_id,user_id)
);
CREATE INDEX folder_shares_recipient ON folder_shares(user_id,folder_id);
