use crate::{
    db::schema::{email_challenges as c, token_blacklist as b, users as u},
    models::{blacklisted_token::BlacklistedToken, email_challenge::EmailChallenge, user::User},
};
use diesel::prelude::*;

pub fn user_by_email(conn: &mut SqliteConnection, email: &str) -> QueryResult<Option<User>> {
    u::table.filter(u::email.eq(email)).first(conn).optional()
}

pub fn user(conn: &mut SqliteConnection, id: &str) -> QueryResult<User> {
    u::table.find(id).first(conn)
}

pub fn insert_user(conn: &mut SqliteConnection, user: &User) -> QueryResult<usize> {
    diesel::insert_into(u::table).values(user).execute(conn)
}

pub fn activate(conn: &mut SqliteConnection, id: &str) -> QueryResult<usize> {
    diesel::update(u::table.find(id))
        .set(u::verified.eq(true))
        .execute(conn)
}

pub fn update_password(conn: &mut SqliteConnection, id: &str, hash: &str) -> QueryResult<usize> {
    diesel::update(u::table.find(id))
        .set((
            u::password_hash.eq(hash),
            u::token_version.eq(u::token_version + 1),
        ))
        .execute(conn)
}

pub fn challenge(conn: &mut SqliteConnection, id: &str) -> QueryResult<Option<EmailChallenge>> {
    c::table.find(id).first(conn).optional()
}

pub fn challenge_for(
    conn: &mut SqliteConnection,
    user_id: &str,
    purpose: &str,
) -> QueryResult<Option<EmailChallenge>> {
    c::table
        .filter(c::user_id.eq(user_id))
        .filter(c::purpose.eq(purpose))
        .first(conn)
        .optional()
}

pub fn replace_challenge(
    conn: &mut SqliteConnection,
    challenge: &EmailChallenge,
) -> QueryResult<usize> {
    diesel::delete(
        c::table
            .filter(c::user_id.eq(&challenge.user_id))
            .filter(c::purpose.eq(&challenge.purpose)),
    )
    .execute(conn)?;
    diesel::insert_into(c::table)
        .values(challenge)
        .execute(conn)
}

pub fn failed_attempt(conn: &mut SqliteConnection, id: &str) -> QueryResult<usize> {
    diesel::update(c::table.find(id))
        .set(c::attempts.eq(c::attempts + 1))
        .execute(conn)
}

pub fn delete_challenge(conn: &mut SqliteConnection, id: &str) -> QueryResult<usize> {
    diesel::delete(c::table.find(id)).execute(conn)
}

pub fn delete_user_challenges(conn: &mut SqliteConnection, user_id: &str) -> QueryResult<usize> {
    diesel::delete(c::table.filter(c::user_id.eq(user_id))).execute(conn)
}

pub fn blacklist_token(
    conn: &mut SqliteConnection,
    token: &BlacklistedToken,
) -> QueryResult<usize> {
    diesel::insert_into(b::table)
        .values(token)
        .on_conflict_do_nothing()
        .execute(conn)
}

pub fn count_old_blacklisted_tokens(conn: &mut SqliteConnection, cutoff: i64) -> QueryResult<i64> {
    b::table
        .filter(b::expires_at.lt(cutoff))
        .count()
        .get_result(conn)
}

pub fn clear_old_blacklisted_tokens(
    conn: &mut SqliteConnection,
    cutoff: i64,
) -> QueryResult<usize> {
    diesel::delete(b::table.filter(b::expires_at.lt(cutoff))).execute(conn)
}
