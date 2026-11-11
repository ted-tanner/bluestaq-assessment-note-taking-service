use crate::db::schema::folders;
use diesel::{Insertable, Queryable, Selectable};
use serde::Serialize;

#[derive(Debug, Serialize, Queryable, Selectable, Insertable)]
#[diesel(table_name = folders)]
pub struct Folder {
    pub id: String,
    #[serde(skip_serializing)]
    pub user_id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub revision: i64,
}
