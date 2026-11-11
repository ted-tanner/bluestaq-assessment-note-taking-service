use crate::{db::schema::notes as n, models::note::Note};
use diesel::prelude::*;

pub fn insert(conn: &mut SqliteConnection, note: &Note) -> QueryResult<usize> {
    diesel::insert_into(n::table).values(note).execute(conn)
}

pub fn get(conn: &mut SqliteConnection, owner: &str, id: &str) -> QueryResult<Note> {
    scoped(conn, owner, None, false)?
        .filter(n::id.eq(id))
        .first(conn)
}

pub fn list(
    conn: &mut SqliteConnection,
    owner: &str,
    limit: i64,
    offset: i64,
) -> QueryResult<Vec<Note>> {
    list_scoped(conn, owner, limit, offset, None, false)
}

pub fn update(conn: &mut SqliteConnection, note: &Note) -> QueryResult<usize> {
    diesel::update(n::table.find(&note.id).filter(n::user_id.eq(&note.user_id)))
        .set((
            n::title.eq(&note.title),
            n::body.eq(&note.body),
            n::updated_at.eq(note.updated_at),
            n::revision.eq(note.revision),
            n::folder_id.eq(&note.folder_id),
        ))
        .execute(conn)
}

pub fn delete(conn: &mut SqliteConnection, owner: &str, id: &str) -> QueryResult<usize> {
    diesel::delete(n::table.find(id).filter(n::user_id.eq(owner))).execute(conn)
}

// FTS5 operators and ranking are exposed as typed Diesel expressions below.
pub fn search(
    conn: &mut SqliteConnection,
    owner: &str,
    expression: &str,
    phrase: &str,
    limit: i64,
    offset: i64,
) -> QueryResult<Vec<Note>> {
    search_scoped(conn, owner, expression, phrase, limit, offset, None, false)
}

/// Access and scope are applied in SQL before pagination or search ranking.
fn scoped<'a>(
    conn: &mut SqliteConnection,
    viewer: &'a str,
    folder: Option<&str>,
    descendants: bool,
) -> QueryResult<n::BoxedQuery<'a, diesel::sqlite::Sqlite>> {
    use super::{access::FolderAccess, schema::note_shares as s};
    let access = FolderAccess::load(conn, viewer)?;
    let mut query = n::table
        .filter(
            n::user_id
                .eq(viewer)
                .or(n::id.eq_any(s::table.filter(s::user_id.eq(viewer)).select(s::note_id)))
                .or(n::folder_id.eq_any(access.permissions.keys().cloned().collect::<Vec<_>>())),
        )
        .into_boxed();
    match folder {
        None => {}
        Some("root") => query = query.filter(n::folder_id.is_null().and(n::user_id.eq(viewer))),
        Some("shared") => query = query.filter(n::user_id.ne(viewer)),
        Some(id) => query = query.filter(n::folder_id.eq_any(access.subtree(id, descendants))),
    }
    Ok(query)
}

pub fn list_scoped(
    conn: &mut SqliteConnection,
    owner: &str,
    limit: i64,
    offset: i64,
    folder: Option<&str>,
    descendants: bool,
) -> QueryResult<Vec<Note>> {
    scoped(conn, owner, folder, descendants)?
        .order((n::updated_at.desc(), n::id.asc()))
        .limit(limit)
        .offset(offset)
        .load(conn)
}

// SQLite already supplies these FTS5 operations; these declarations only give
// Diesel their types. User search text is still bound as a parameter.
diesel::infix_operator!(Matches, " MATCH ", diesel::sql_types::Bool, backend: diesel::sqlite::Sqlite);

diesel::define_sql_function! {
    fn bm25(index: diesel::sql_types::Text, title: diesel::sql_types::Double, body: diesel::sql_types::Double) -> diesel::sql_types::Double;
}

#[allow(clippy::too_many_arguments)]
pub fn search_scoped(
    conn: &mut SqliteConnection,
    owner: &str,
    expression: &str,
    phrase: &str,
    limit: i64,
    offset: i64,
    folder: Option<&str>,
    descendants: bool,
) -> QueryResult<Vec<Note>> {
    use super::schema::{note_rowids as r, notes_fts as fts};
    use diesel::expression::AsExpression;
    use diesel::sql_types::Text;
    let rows = diesel::alias!(super::schema::note_rowids as indexed_note);
    let phrase_fts = diesel::alias!(super::schema::notes_fts as phrase_fts);
    let phrase_matches = phrase_fts
        .select(phrase_fts.field(fts::rowid))
        .filter(Matches::new(
            phrase_fts.field(fts::index),
            <&str as AsExpression<Text>>::as_expression(phrase),
        ));
    scoped(conn, owner, folder, descendants)?
        .inner_join(rows.on(rows.field(r::id).eq(n::id)))
        .inner_join(fts::table.on(fts::rowid.eq(rows.field(r::rowid))))
        .filter(Matches::new(
            fts::index,
            <&str as AsExpression<Text>>::as_expression(expression),
        ))
        .order((
            fts::rowid.eq_any(phrase_matches).desc(),
            bm25(fts::index, 8.0, 1.0),
            n::updated_at.desc(),
            n::id.asc(),
        ))
        .select(Note::as_select())
        .limit(limit)
        .offset(offset)
        .load(conn)
}
