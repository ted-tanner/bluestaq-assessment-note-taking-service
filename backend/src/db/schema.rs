diesel::table! {
    users (id) {
        id -> Text,
        email -> Text,
        password_hash -> Text,
        token_version -> BigInt,
        verified -> Bool,
        created_at -> BigInt,
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
    }
}

diesel::table! {
    token_blacklist (token_hash) {
        token_hash -> Text,
        expires_at -> BigInt,
    }
}
