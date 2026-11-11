use crate::db::schema::notes;
use diesel::{Insertable, Queryable, Selectable};
use serde::Serialize;

#[derive(Debug, Serialize, Queryable, Selectable, Insertable)]
#[diesel(table_name = notes)]
pub struct Note {
    pub id: String,
    #[serde(skip_serializing)]
    pub user_id: String,
    pub title: String,
    pub body: String,
    pub created_at: i64,
    pub updated_at: i64,
}
