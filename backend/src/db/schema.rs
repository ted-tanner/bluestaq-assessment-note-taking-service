diesel::table! {
    users (id) {
        id -> Text,
        email -> Text,
        password_hash -> Text,
        token_version -> BigInt,
        verified -> Bool,
        created_at -> BigInt,
        name -> Text,
        name_key -> Text,
    }
}

diesel::table! {
    email_challenges (id) {
        id -> Text,
        user_id -> Text,
        purpose -> Text,
        code_hash -> Text,
        expires_at -> BigInt,
        attempts -> Integer,
        last_sent_at -> BigInt,
        created_at -> BigInt,
    }
}

diesel::table! {
    notes (id) {
        id -> Text,
        user_id -> Text,
        title -> Text,
        body -> Text,
        created_at -> BigInt,
        updated_at -> BigInt,
        revision -> BigInt,
        folder_id -> Nullable<Text>,
    }
}

diesel::table! {
    token_blacklist (token_hash) {
        token_hash -> Text,
        expires_at -> BigInt,
    }
}

diesel::table! {
    folders (id) {
        id -> Text,
        user_id -> Text,
        name -> Text,
        parent_id -> Nullable<Text>,
        created_at -> BigInt,
        updated_at -> BigInt,
        revision -> BigInt,
    }
}

diesel::table! {
 note_shares (note_id, user_id) { note_id -> Text, user_id -> Text, permission -> Text, }
}
diesel::table! {
 folder_shares (folder_id, user_id) { folder_id -> Text, user_id -> Text, permission -> Text, }
}

// Read-only projections for SQLite's implicit rowid and FTS5 hidden column.
diesel::table! {
    #[sql_name = "notes"]
    note_rowids (rowid) {
        rowid -> BigInt,
        id -> Text,
    }
}

diesel::table! {
    notes_fts (rowid) {
        rowid -> BigInt,
        #[sql_name = "notes_fts"]
        index -> Text,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    notes,
    note_shares,
    folders,
    folder_shares,
    users,
    note_rowids,
    notes_fts,
);
