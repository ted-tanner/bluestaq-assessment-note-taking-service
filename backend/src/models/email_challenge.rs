use crate::db::schema::email_challenges;
use diesel::{Insertable, Queryable, Selectable};

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = email_challenges)]
pub struct EmailChallenge {
    pub id: String,
    pub user_id: String,
    pub purpose: String,
    pub code_hash: String,
    pub expires_at: i64,
    pub attempts: i32,
    pub last_sent_at: i64,
    pub created_at: i64,
}
