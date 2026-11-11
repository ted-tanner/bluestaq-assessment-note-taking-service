use super::schema::users as u;
use crate::models::user::UserInfo;
use diesel::prelude::*;

pub fn get(conn: &mut SqliteConnection, id: &str) -> QueryResult<UserInfo> {
    u::table.find(id).select(UserInfo::as_select()).first(conn)
}

pub fn rename(conn: &mut SqliteConnection, id: &str, name: &str) -> QueryResult<usize> {
    diesel::update(u::table.find(id))
        .set((u::name.eq(name), u::name_key.eq(name.to_lowercase())))
        .execute(conn)
}

pub fn search(
    conn: &mut SqliteConnection,
    viewer: &str,
    query: &str,
) -> QueryResult<Vec<UserInfo>> {
    let escaped = query
        .to_lowercase()
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let pattern = format!("%{escaped}%");
    u::table
        .filter(u::verified.eq(true))
        .filter(u::id.ne(viewer))
        .filter(
            u::name_key
                .like(&pattern)
                .escape('\\')
                .or(u::email.like(&pattern).escape('\\')),
        )
        .order((u::name_key.asc(), u::email.asc()))
        .limit(20)
        .select(UserInfo::as_select())
        .load(conn)
}
