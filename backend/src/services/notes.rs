use crate::{
    db::{self, DbPool, notes as dao},
    error::Error,
    models::note::Note,
    utils::crypto,
};
use serde::Deserialize;
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
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NotePatch {
    pub title: Option<String>,
    pub body: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pagination {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    50
}

impl NoteService {
    pub async fn create(&self, owner: String, input: NewNote) -> Result<Note, Error> {
        validate(&input.title, &input.body)?;

        db::run(self.pool.clone(), move |conn| {
            let now = crypto::now();
            let note = Note {
                id: Uuid::new_v4().to_string(),
                user_id: owner,
                title: input.title,
                body: input.body,
                created_at: now,
                updated_at: now,
            };

            dao::insert(conn, &note)?;

            Ok(note)
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
            Ok(dao::list(conn, &owner, page.limit, page.offset)?)
        })
        .await
    }

    pub async fn update(&self, owner: String, id: String, patch: NotePatch) -> Result<Note, Error> {
        if patch.title.is_none() && patch.body.is_none() {
            return Err(Error::BadRequest("Provide title or body"));
        }

        db::run(self.pool.clone(), move |conn| {
            conn.immediate_transaction::<_, Error, _>(|conn| {
                let mut note = dao::get(conn, &owner, &id)?;
                if let Some(title) = patch.title {
                    note.title = title;
                }

                if let Some(body) = patch.body {
                    note.body = body;
                }

                validate(&note.title, &note.body)?;
                note.updated_at = crypto::now();
                dao::update(conn, &note)?;

                Ok(note)
            })
        })
        .await
    }

    pub async fn delete(&self, owner: String, id: String) -> Result<(), Error> {
        db::run(self.pool.clone(), move |conn| {
            if dao::delete(conn, &owner, &id)? == 0 {
                return Err(Error::NotFound);
            }

            Ok(())
        })
        .await
    }
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
                    title: None,
                    body: Some("updated".into()),
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
                    title: Some(String::new()),
                    body: None,
                },
            )
            .await
            .unwrap();
        assert!(cleared.title.is_empty());
        assert_eq!(cleared.body, "updated");
        notes.create("alice".into(), input("", "")).await.unwrap();
        notes.delete("alice".into(), note.id.clone()).await.unwrap();
        assert!(matches!(
            notes.get("alice".into(), note.id.clone()).await,
            Err(Error::NotFound)
        ));
        assert!(matches!(
            notes.delete("alice".into(), note.id).await,
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
                            title: Some("stolen".into()),
                            body: None
                        }
                    )
                    .await,
                Err(Error::NotFound)
            ));
            assert!(matches!(
                notes.delete("bob".into(), id).await,
                Err(Error::NotFound)
            ));
        }
        assert!(
            notes
                .list(
                    "bob".into(),
                    Pagination {
                        limit: 50,
                        offset: 0
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
                title: None,
                body: None,
            },
            NotePatch {
                title: Some("é".repeat(256)),
                body: Some("changed".into()),
            },
            NotePatch {
                title: Some("changed".into()),
                body: Some(format!("{body}x")),
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
                        offset: 0
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
                        offset: 3
                    }
                )
                .await
                .unwrap()
                .is_empty()
        );
        for (limit, offset) in [(0, 0), (101, 0), (1, -1)] {
            assert!(matches!(
                notes
                    .list("alice".into(), Pagination { limit, offset })
                    .await,
                Err(Error::BadRequest(_))
            ));
        }
    }
}
