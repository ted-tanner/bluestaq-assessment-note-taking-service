use crate::{db::schema::notes as n, models::note::Note};
use diesel::prelude::*;

pub fn insert(conn: &mut SqliteConnection, note: &Note) -> QueryResult<usize> {
    diesel::insert_into(n::table).values(note).execute(conn)
}

pub fn get(conn: &mut SqliteConnection, owner: &str, id: &str) -> QueryResult<Note> {
    n::table.find(id).filter(n::user_id.eq(owner)).first(conn)
}

pub fn list(
    conn: &mut SqliteConnection,
    owner: &str,
    limit: i64,
    offset: i64,
) -> QueryResult<Vec<Note>> {
    n::table
        .filter(n::user_id.eq(owner))
        .order((n::updated_at.desc(), n::id.asc()))
        .limit(limit)
        .offset(offset)
        .load(conn)
}

pub fn update(conn: &mut SqliteConnection, note: &Note) -> QueryResult<usize> {
    diesel::update(n::table.find(&note.id).filter(n::user_id.eq(&note.user_id)))
        .set((
            n::title.eq(&note.title),
            n::body.eq(&note.body),
            n::updated_at.eq(note.updated_at),
        ))
        .execute(conn)
}

pub fn delete(conn: &mut SqliteConnection, owner: &str, id: &str) -> QueryResult<usize> {
    diesel::delete(n::table.find(id).filter(n::user_id.eq(owner))).execute(conn)
}
