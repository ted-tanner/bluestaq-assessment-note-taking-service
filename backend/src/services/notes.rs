use crate::{
    db::{self, DbPool, notes as dao},
    error::Error,
    models::note::Note,
    utils::crypto,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone)]
pub struct NoteService {
    pub pool: DbPool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewNote {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
    pub folder_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NotePatch {
    #[serde(skip)]
    pub expected_revision: i64,
    pub title: Option<String>,
    pub body: Option<String>,
    #[serde(default, deserialize_with = "super::folders::relationship")]
    pub folder_id: Option<Option<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pagination {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    pub folder_id: Option<String>,
    #[serde(default)]
    pub include_descendants: bool,
}

#[derive(Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    pub q: String,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    pub folder_id: Option<String>,
    #[serde(default)]
    pub include_descendants: bool,
}

#[derive(Serialize)]
pub struct SearchResults {
    pub items: Vec<Note>,
    pub has_more: bool,
}

fn default_limit() -> i64 {
    50
}

impl NoteService {
    pub async fn create(&self, owner: String, input: NewNote) -> Result<Note, Error> {
        validate(&input.title, &input.body)?;

        db::run(self.pool.clone(), move |conn| {
            conn.immediate_transaction::<_, Error, _>(|conn| {
                let note_owner = if let Some(folder) = input.folder_id.as_deref() {
                    super::sharing::require_edit(db::sharing::folder_permission(
                        conn, &owner, folder,
                    )?)?;
                    db::sharing::readable_folder(conn, &owner, folder)?.user_id
                } else {
                    owner.clone()
                };
                let now = crypto::now();
                let note = Note {
                    id: Uuid::new_v4().to_string(),
                    user_id: note_owner,
                    title: input.title,
                    body: input.body,
                    created_at: now,
                    updated_at: now,
                    revision: 1,
                    folder_id: input.folder_id,
                };

                dao::insert(conn, &note)?;

                Ok(note)
            })
        })
        .await
    }

    pub async fn get(&self, owner: String, id: String) -> Result<Note, Error> {
        db::run(self.pool.clone(), move |conn| {
            Ok(dao::get(conn, &owner, &id)?)
        })
        .await
    }

    pub async fn list(&self, owner: String, page: Pagination) -> Result<Vec<Note>, Error> {
        if !(1..=100).contains(&page.limit) || page.offset < 0 {
            return Err(Error::BadRequest(
                "limit must be 1–100 and offset must be nonnegative",
            ));
        }

        db::run(self.pool.clone(), move |conn| {
            validate_scope(conn, &owner, page.folder_id.as_deref())?;
            Ok(dao::list_scoped(
                conn,
                &owner,
                page.limit,
                page.offset,
                page.folder_id.as_deref(),
                page.include_descendants,
            )?)
        })
        .await
    }

    pub async fn update(&self, owner: String, id: String, patch: NotePatch) -> Result<Note, Error> {
        if patch.title.is_none() && patch.body.is_none() && patch.folder_id.is_none() {
            return Err(Error::BadRequest("Provide title, body, or folder_id"));
        }

        db::run(self.pool.clone(), move |conn| {
            conn.immediate_transaction::<_, Error, _>(|conn| {
                let mut note = dao::get(conn, &owner, &id)?;
                super::sharing::require_edit(db::sharing::note_permission(conn, &owner, &id)?)?;
                if patch.expected_revision != note.revision {
                    return Err(Error::PreconditionFailed);
                }
                if let Some(title) = patch.title {
                    note.title = title;
                }

                if let Some(body) = patch.body {
                    note.body = body;
                }

                if let Some(folder_id) = patch.folder_id {
                    if note.user_id != owner && folder_id != note.folder_id {
                        return Err(Error::Forbidden);
                    }
                    if folder_id != note.folder_id {
                        super::folders::validate_destination(conn, &owner, folder_id.as_deref())?;
                    }
                    note.folder_id = folder_id;
                }
                validate(&note.title, &note.body)?;
                note.updated_at = crypto::now();
                note.revision += 1;
                dao::update(conn, &note)?;

                Ok(note)
            })
        })
        .await
    }

    pub async fn delete(
        &self,
        owner: String,
        id: String,
        expected_revision: i64,
    ) -> Result<(), Error> {
        db::run(self.pool.clone(), move |conn| {
            conn.immediate_transaction::<_, Error, _>(|conn| {
                let note = dao::get(conn, &owner, &id)?;
                if note.user_id != owner {
                    return Err(Error::Forbidden);
                }
                if expected_revision != note.revision {
                    return Err(Error::PreconditionFailed);
                }
                dao::delete(conn, &owner, &id)?;
                Ok(())
            })
        })
        .await
    }

    pub async fn search(&self, owner: String, query: SearchQuery) -> Result<SearchResults, Error> {
        if !(1..=100).contains(&query.limit) || query.offset < 0 || query.q.chars().count() > 256 {
            return Err(Error::BadRequest("Invalid search length or pagination"));
        }
        let terms: Vec<_> = query
            .q
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .collect();
        if terms.len() > 20 {
            return Err(Error::BadRequest("Search supports at most 20 terms"));
        }
        let expression = terms
            .iter()
            .enumerate()
            .map(|(i, term)| format!("\"{term}\"{}", if i + 1 == terms.len() { "*" } else { "" }))
            .collect::<Vec<_>>()
            .join(" AND ");
        let phrase = format!("title : \"{}\"", terms.join(" "));
        db::run(self.pool.clone(), move |conn| {
            validate_scope(conn, &owner, query.folder_id.as_deref())?;
            let mut items = if query.q.trim().is_empty() {
                dao::list_scoped(
                    conn,
                    &owner,
                    query.limit + 1,
                    query.offset,
                    query.folder_id.as_deref(),
                    query.include_descendants,
                )?
            } else if expression.is_empty() {
                vec![]
            } else {
                dao::search_scoped(
                    conn,
                    &owner,
                    &expression,
                    &phrase,
                    query.limit + 1,
                    query.offset,
                    query.folder_id.as_deref(),
                    query.include_descendants,
                )?
            };
            let has_more = items.len() > query.limit as usize;
            items.truncate(query.limit as usize);
            Ok(SearchResults { items, has_more })
        })
        .await
    }
}

fn validate_scope(
    conn: &mut diesel::SqliteConnection,
    owner: &str,
    folder: Option<&str>,
) -> Result<(), Error> {
    if let Some(id) = folder.filter(|id| !["root", "shared"].contains(id)) {
        db::sharing::folder_permission(conn, owner, id)?;
    }
    Ok(())
}

fn validate(title: &str, body: &str) -> Result<(), Error> {
    if title.chars().count() > 255 {
        return Err(Error::BadRequest("Title exceeds 255 characters"));
    }

    if body.len() > 100 * 1024 {
        return Err(Error::BadRequest("Body exceeds 100 KiB"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::TestDb;
    use diesel::prelude::*;

    fn service(database: &TestDb) -> NoteService {
        database.insert_user("alice");
        database.insert_user("bob");
        NoteService {
            pool: database.pool.clone(),
        }
    }

    fn input(title: &str, body: &str) -> NewNote {
        NewNote {
            title: title.into(),
            body: body.into(),
            folder_id: None,
        }
    }

    #[actix_web::test]
    async fn crud_persists_partial_updates_and_empty_content() {
        let database = TestDb::new();
        let notes = service(&database);
        let note = notes
            .create("alice".into(), input("First", "hello"))
            .await
            .unwrap();
        assert!(Uuid::parse_str(&note.id).is_ok());
        let updated = notes
            .update(
                "alice".into(),
                note.id.clone(),
                NotePatch {
                    expected_revision: 1,
                    title: None,
                    body: Some("updated".into()),
                    folder_id: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(updated.title, "First");
        assert_eq!(updated.body, "updated");
        assert_eq!(updated.created_at, note.created_at);
        assert!(updated.updated_at >= note.updated_at);
        let reopened = NoteService {
            pool: database.reopen(),
        };
        let persisted = reopened.get("alice".into(), note.id.clone()).await.unwrap();
        assert_eq!(persisted.body, "updated");
        let cleared = notes
            .update(
                "alice".into(),
                note.id.clone(),
                NotePatch {
                    expected_revision: 2,
                    title: Some(String::new()),
                    body: None,
                    folder_id: None,
                },
            )
            .await
            .unwrap();
        assert!(cleared.title.is_empty());
        assert_eq!(cleared.body, "updated");
        notes.create("alice".into(), input("", "")).await.unwrap();
        notes
            .delete("alice".into(), note.id.clone(), 3)
            .await
            .unwrap();
        assert!(matches!(
            notes.get("alice".into(), note.id.clone()).await,
            Err(Error::NotFound)
        ));
        assert!(matches!(
            notes.delete("alice".into(), note.id, 3).await,
            Err(Error::NotFound)
        ));
    }

    #[actix_web::test]
    async fn owners_cannot_read_update_delete_or_list_each_others_notes() {
        let database = TestDb::new();
        let notes = service(&database);
        let note = notes
            .create("alice".into(), input("private", "secret"))
            .await
            .unwrap();
        for id in [note.id.clone(), Uuid::new_v4().to_string()] {
            assert!(matches!(
                notes.get("bob".into(), id.clone()).await,
                Err(Error::NotFound)
            ));
            assert!(matches!(
                notes
                    .update(
                        "bob".into(),
                        id.clone(),
                        NotePatch {
                            expected_revision: 1,
                            title: Some("stolen".into()),
                            body: None,
                            folder_id: None,
                        }
                    )
                    .await,
                Err(Error::NotFound)
            ));
            assert!(matches!(
                notes.delete("bob".into(), id, 1).await,
                Err(Error::NotFound)
            ));
        }
        assert!(
            notes
                .list(
                    "bob".into(),
                    Pagination {
                        limit: 50,
                        offset: 0,
                        folder_id: None,
                        include_descendants: false,
                    }
                )
                .await
                .unwrap()
                .is_empty()
        );
        let stored = notes.get("alice".into(), note.id).await.unwrap();
        assert_eq!(stored.title, "private");
        assert_eq!(stored.body, "secret");
    }

    #[actix_web::test]
    async fn validation_uses_unicode_title_characters_and_utf8_body_bytes() {
        let database = TestDb::new();
        let notes = service(&database);
        let title = "é".repeat(255);
        let body = "é".repeat(50 * 1024);
        let note = notes
            .create("alice".into(), input(&title, &body))
            .await
            .unwrap();
        for invalid in [input(&"é".repeat(256), ""), input("", &format!("{body}x"))] {
            assert!(matches!(
                notes.create("alice".into(), invalid).await,
                Err(Error::BadRequest(_))
            ));
        }
        for patch in [
            NotePatch {
                expected_revision: 1,
                title: None,
                body: None,
                folder_id: None,
            },
            NotePatch {
                expected_revision: 1,
                title: Some("é".repeat(256)),
                body: Some("changed".into()),
                folder_id: None,
            },
            NotePatch {
                expected_revision: 1,
                title: Some("changed".into()),
                body: Some(format!("{body}x")),
                folder_id: None,
            },
        ] {
            assert!(matches!(
                notes.update("alice".into(), note.id.clone(), patch).await,
                Err(Error::BadRequest(_))
            ));
            let stored = notes.get("alice".into(), note.id.clone()).await.unwrap();
            assert_eq!(stored.title, title);
            assert_eq!(stored.body, body);
            assert_eq!(stored.updated_at, note.updated_at);
        }
        assert_eq!(
            notes
                .list(
                    "alice".into(),
                    Pagination {
                        limit: 100,
                        offset: 0,
                        folder_id: None,
                        include_descendants: false,
                    }
                )
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[actix_web::test]
    async fn pagination_orders_by_updated_time_then_id_and_checks_bounds() {
        use crate::db::schema::notes as n;
        let database = TestDb::new();
        let notes = service(&database);
        let mut tied = Vec::new();
        for _ in 0..2 {
            let note = notes
                .create("alice".into(), input("tie", ""))
                .await
                .unwrap();
            diesel::update(n::table.find(&note.id))
                .set(n::updated_at.eq(100))
                .execute(&mut database.pool.get().unwrap())
                .unwrap();
            tied.push(note.id);
        }
        tied.sort();
        let newest = notes
            .create("alice".into(), input("newest", ""))
            .await
            .unwrap();
        let all = notes
            .list(
                "alice".into(),
                Pagination {
                    limit: 100,
                    offset: 0,
                    folder_id: None,
                    include_descendants: false,
                },
            )
            .await
            .unwrap();
        assert_eq!(
            all.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
            [newest.id.as_str(), tied[0].as_str(), tied[1].as_str()]
        );
        for (offset, id) in tied.iter().enumerate() {
            let page = notes
                .list(
                    "alice".into(),
                    Pagination {
                        limit: 1,
                        offset: offset as i64 + 1,
                        folder_id: None,
                        include_descendants: false,
                    },
                )
                .await
                .unwrap();
            assert_eq!(&page[0].id, id);
        }
        assert!(
            notes
                .list(
                    "alice".into(),
                    Pagination {
                        limit: 50,
                        offset: 3,
                        folder_id: None,
                        include_descendants: false,
                    }
                )
                .await
                .unwrap()
                .is_empty()
        );
        for (limit, offset) in [(0, 0), (101, 0), (1, -1)] {
            assert!(matches!(
                notes
                    .list(
                        "alice".into(),
                        Pagination {
                            limit,
                            offset,
                            folder_id: None,
                            include_descendants: false,
                        }
                    )
                    .await,
                Err(Error::BadRequest(_))
            ));
        }
    }

    #[actix_web::test]
    async fn search_ranks_titles_handles_unicode_and_tracks_changes() {
        let database = TestDb::new();
        let notes = service(&database);
        let exact = notes
            .create("alice".into(), input("Café project", "planning details"))
            .await
            .unwrap();
        let body = notes
            .create("alice".into(), input("Other", "café project café project"))
            .await
            .unwrap();
        notes
            .create("bob".into(), input("Café project", "private"))
            .await
            .unwrap();
        let result = notes
            .search(
                "alice".into(),
                SearchQuery {
                    q: "CAFE project".into(),
                    limit: 1,
                    offset: 0,
                    folder_id: None,
                    include_descendants: false,
                },
            )
            .await
            .unwrap();
        assert_eq!(result.items[0].id, exact.id);
        assert!(result.has_more);
        let second = notes
            .search(
                "alice".into(),
                SearchQuery {
                    q: "cafe proj".into(),
                    limit: 1,
                    offset: 1,
                    folder_id: None,
                    include_descendants: false,
                },
            )
            .await
            .unwrap();
        assert_eq!(second.items[0].id, body.id);
        assert!(!second.has_more);
        for query in ["* : \"", "unmatched", "cafe absent", "OR NOT NEAR"] {
            assert!(
                notes
                    .search(
                        "alice".into(),
                        SearchQuery {
                            q: query.into(),
                            limit: 50,
                            offset: 0,
                            folder_id: None,
                            include_descendants: false,
                        }
                    )
                    .await
                    .unwrap()
                    .items
                    .is_empty()
            );
        }
        let updated = notes
            .update(
                "alice".into(),
                exact.id.clone(),
                NotePatch {
                    expected_revision: 1,
                    title: Some("Renamed".into()),
                    body: Some("newtoken".into()),
                    folder_id: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(updated.revision, 2);
        assert_eq!(
            notes
                .search(
                    "alice".into(),
                    SearchQuery {
                        q: "newtok".into(),
                        limit: 50,
                        offset: 0,
                        folder_id: None,
                        include_descendants: false,
                    }
                )
                .await
                .unwrap()
                .items[0]
                .id,
            exact.id
        );
        notes.delete("alice".into(), exact.id, 2).await.unwrap();
        assert!(
            notes
                .search(
                    "alice".into(),
                    SearchQuery {
                        q: "newtok".into(),
                        limit: 50,
                        offset: 0,
                        folder_id: None,
                        include_descendants: false,
                    }
                )
                .await
                .unwrap()
                .items
                .is_empty()
        );
        assert_eq!(
            notes
                .search(
                    "bob".into(),
                    SearchQuery {
                        q: "".into(),
                        limit: 50,
                        offset: 0,
                        folder_id: None,
                        include_descendants: false,
                    }
                )
                .await
                .unwrap()
                .items
                .len(),
            1
        );
        for query in [
            SearchQuery {
                q: "a".repeat(257),
                limit: 1,
                offset: 0,
                folder_id: None,
                include_descendants: false,
            },
            SearchQuery {
                q: "a ".repeat(21),
                limit: 1,
                offset: 0,
                folder_id: None,
                include_descendants: false,
            },
            SearchQuery {
                q: "x".into(),
                limit: 0,
                offset: 0,
                folder_id: None,
                include_descendants: false,
            },
            SearchQuery {
                q: "x".into(),
                limit: 1,
                offset: -1,
                folder_id: None,
                include_descendants: false,
            },
        ] {
            assert!(matches!(
                notes.search("alice".into(), query).await,
                Err(Error::BadRequest(_))
            ));
        }
    }

    #[actix_web::test]
    async fn concurrent_updates_have_one_winner_and_stale_deletes_preserve_content() {
        let database = TestDb::new();
        let notes = service(&database);
        let note = notes
            .create("alice".into(), input("original", "body"))
            .await
            .unwrap();
        let first = notes.update(
            "alice".into(),
            note.id.clone(),
            NotePatch {
                expected_revision: 1,
                title: Some("first".into()),
                body: None,
                folder_id: None,
            },
        );
        let second = notes.update(
            "alice".into(),
            note.id.clone(),
            NotePatch {
                expected_revision: 1,
                title: Some("second".into()),
                body: None,
                folder_id: None,
            },
        );
        let (a, b) = tokio::join!(first, second);
        assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
        assert!(
            matches!(a, Err(Error::PreconditionFailed))
                || matches!(b, Err(Error::PreconditionFailed))
        );
        assert!(matches!(
            notes.delete("alice".into(), note.id.clone(), 1).await,
            Err(Error::PreconditionFailed)
        ));
        let stored = notes.get("alice".into(), note.id).await.unwrap();
        assert_eq!(stored.revision, 2);
        assert_eq!(stored.body, "body");
    }
}
