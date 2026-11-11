use crate::db::schema::token_blacklist;
use diesel::{Insertable, Queryable, Selectable};

#[derive(Insertable, Queryable, Selectable)]
#[diesel(table_name = token_blacklist)]
pub struct BlacklistedToken {
    pub token_hash: String,
    pub expires_at: i64,
}
