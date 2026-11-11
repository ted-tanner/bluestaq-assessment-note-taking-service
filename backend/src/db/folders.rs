use super::schema::{folders as f, notes as n};
use crate::models::folder::Folder;
use diesel::prelude::*;

pub fn get(conn: &mut SqliteConnection, id: &str) -> QueryResult<Folder> {
    f::table.find(id).first(conn)
}

pub fn owned(conn: &mut SqliteConnection, owner: &str, id: &str) -> QueryResult<Folder> {
    f::table.find(id).filter(f::user_id.eq(owner)).first(conn)
}

pub fn list(conn: &mut SqliteConnection) -> QueryResult<Vec<Folder>> {
    f::table.order((f::name, f::id)).load(conn)
}

pub fn insert(conn: &mut SqliteConnection, folder: &Folder) -> QueryResult<usize> {
    diesel::insert_into(f::table).values(folder).execute(conn)
}

pub fn update(conn: &mut SqliteConnection, folder: &Folder) -> QueryResult<usize> {
    diesel::update(f::table.find(&folder.id))
        .set((
            f::name.eq(&folder.name),
            f::parent_id.eq(&folder.parent_id),
            f::revision.eq(folder.revision),
            f::updated_at.eq(folder.updated_at),
        ))
        .execute(conn)
}

/// Caller holds the write transaction so notes and folders are removed atomically.
pub fn delete_subtree(c: &mut SqliteConnection, owner: &str, id: &str) -> QueryResult<()> {
    // Materialize the subtree once and delete children before their parents.
    let all: Vec<Folder> = f::table.filter(f::user_id.eq(owner)).load(c)?;
    let mut children = std::collections::HashMap::<String, Vec<String>>::new();
    for folder in all {
        if let Some(parent) = folder.parent_id {
            children.entry(parent).or_default().push(folder.id);
        }
    }
    let mut ids = vec![id.to_owned()];
    let mut cursor = 0;
    while cursor < ids.len() {
        if let Some(nested) = children.remove(&ids[cursor]) {
            ids.extend(nested);
        }
        cursor += 1;
    }
    for id in ids.iter().rev() {
        // Delete per folder to avoid SQLite's bind-parameter limit on large trees.
        diesel::delete(
            n::table
                .filter(n::user_id.eq(owner))
                .filter(n::folder_id.eq(id)),
        )
        .execute(c)?;
        diesel::delete(f::table.find(id).filter(f::user_id.eq(owner))).execute(c)?;
    }
    Ok(())
}
