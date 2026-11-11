use crate::db::schema::users;
use diesel::{Insertable, Queryable, Selectable};
use serde::Serialize;

#[derive(Clone, Queryable, Selectable, Insertable)]
#[diesel(table_name = users)]
pub struct User {
    pub id: String,
    pub email: String,
    pub password_hash: String,
    pub token_version: i64,
    pub verified: bool,
    pub created_at: i64,
}

#[derive(Clone, Serialize)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
}

impl From<User> for UserInfo {
    fn from(user: User) -> Self {
        Self {
            id: user.id,
            email: user.email,
        }
    }
}
