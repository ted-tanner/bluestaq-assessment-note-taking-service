use crate::{
    db::{self, DbPool, folders as dao},
    error::Error,
    models::folder::Folder,
    utils::crypto,
};
use diesel::SqliteConnection;
use serde::Deserialize;
use uuid::Uuid;

// A present JSON null clears the relationship; an absent field leaves it unchanged.
pub fn relationship<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(d).map(Some)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewFolder {
    pub name: String,
    pub parent_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FolderPatch {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "relationship")]
    pub parent_id: Option<Option<String>>,
}

#[derive(Clone)]
pub struct FolderService {
    pub pool: DbPool,
}

pub fn validate_destination(
    conn: &mut SqliteConnection,
    owner: &str,
    id: Option<&str>,
) -> Result<(), Error> {
    if let Some(id) = id {
        dao::owned(conn, owner, id)?;
    }
    Ok(())
}

fn name(value: String) -> Result<String, Error> {
    let value = value.trim().to_owned();
    if !(1..=255).contains(&value.chars().count()) {
        return Err(Error::BadRequest(
            "Folder name must contain 1–255 characters",
        ));
    }
    Ok(value)
}

impl FolderService {
    pub async fn list(&self, owner: String) -> Result<Vec<Folder>, Error> {
        db::run(self.pool.clone(), move |c| {
            db::sharing::visible_folders(c, &owner)
        })
        .await
    }

    pub async fn create(&self, owner: String, input: NewFolder) -> Result<Folder, Error> {
        let name = name(input.name)?;
        db::run(self.pool.clone(), move |c| {
            c.immediate_transaction::<_, Error, _>(|c| {
                let folder_owner = if let Some(parent) = input.parent_id.as_deref() {
                    super::sharing::require_edit(db::sharing::folder_permission(
                        c, &owner, parent,
                    )?)?;
                    db::sharing::readable_folder(c, &owner, parent)?.user_id
                } else {
                    owner.clone()
                };
                let now = crypto::now();
                let folder = Folder {
                    id: Uuid::new_v4().to_string(),
                    user_id: folder_owner,
                    name,
                    parent_id: input.parent_id,
                    created_at: now,
                    updated_at: now,
                    revision: 1,
                };
                dao::insert(c, &folder)?;
                Ok(folder)
            })
        })
        .await
    }

    pub async fn update(
        &self,
        owner: String,
        id: String,
        revision: i64,
        patch: FolderPatch,
    ) -> Result<Folder, Error> {
        if patch.name.is_none() && patch.parent_id.is_none() {
            return Err(Error::BadRequest("Provide name or parent_id"));
        }
        db::run(self.pool.clone(), move |c| {
            c.immediate_transaction::<_, Error, _>(|c| {
                let mut folder = db::sharing::readable_folder(c, &owner, &id)?;
                super::sharing::require_edit(db::sharing::folder_permission(c, &owner, &id)?)?;
                if folder.revision != revision {
                    return Err(Error::PreconditionFailed);
                }
                if let Some(value) = patch.name {
                    folder.name = name(value)?;
                }
                if let Some(parent) = patch.parent_id {
                    if folder.user_id != owner {
                        return Err(Error::Forbidden);
                    }
                    validate_destination(c, &owner, parent.as_deref())?;
                    let mut ancestor = parent.clone();
                    while let Some(current) = ancestor {
                        if current == id {
                            return Err(Error::BadRequest(
                                "A folder cannot be moved into itself or its descendants",
                            ));
                        }
                        ancestor = dao::owned(c, &owner, &current)?.parent_id;
                    }
                    folder.parent_id = parent;
                }
                folder.revision += 1;
                folder.updated_at = crypto::now();
                dao::update(c, &folder)?;
                Ok(folder)
            })
        })
        .await
    }

    pub async fn delete(&self, owner: String, id: String, revision: i64) -> Result<(), Error> {
        db::run(self.pool.clone(), move |c| {
            c.immediate_transaction::<_, Error, _>(|c| {
                let folder = db::sharing::readable_folder(c, &owner, &id)?;
                if folder.user_id != owner {
                    return Err(Error::Forbidden);
                }
                if folder.revision != revision {
                    return Err(Error::PreconditionFailed);
                }

                dao::delete_subtree(c, &owner, &id)?;
                Ok(())
            })
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        services::notes::{NewNote, NoteService},
        test_utils::TestDb,
    };
    use diesel::connection::SimpleConnection;

    #[actix_web::test]
    async fn failed_subtree_deletion_rolls_back_notes_folders_and_search() {
        let database = TestDb::new();
        database.insert_user("owner");
        let folders = FolderService {
            pool: database.pool.clone(),
        };
        let notes = NoteService {
            pool: database.pool.clone(),
        };
        let root = folders
            .create(
                "owner".into(),
                NewFolder {
                    name: "Root".into(),
                    parent_id: None,
                },
            )
            .await
            .unwrap();
        let child = folders
            .create(
                "owner".into(),
                NewFolder {
                    name: "Child".into(),
                    parent_id: Some(root.id.clone()),
                },
            )
            .await
            .unwrap();
        let note = notes
            .create(
                "owner".into(),
                NewNote {
                    title: "searchable".into(),
                    body: "keep me".into(),
                    folder_id: Some(child.id),
                },
            )
            .await
            .unwrap();
        database.pool.get().unwrap().batch_execute("CREATE TRIGGER fail_folder_delete BEFORE DELETE ON folders WHEN old.parent_id IS NULL BEGIN SELECT RAISE(ABORT, 'test rollback'); END;").unwrap();
        assert!(
            folders
                .delete("owner".into(), root.id.clone(), 1)
                .await
                .is_err()
        );
        assert_eq!(folders.list("owner".into()).await.unwrap().len(), 2);
        assert_eq!(
            notes.get("owner".into(), note.id).await.unwrap().body,
            "keep me"
        );
        assert_eq!(
            crate::db::notes::search(
                &mut database.pool.get().unwrap(),
                "owner",
                "\"searchable\"*",
                "title : \"searchable\"",
                50,
                0
            )
            .unwrap()
            .len(),
            1
        );
        database
            .pool
            .get()
            .unwrap()
            .batch_execute("DROP TRIGGER fail_folder_delete;")
            .unwrap();
        folders.delete("owner".into(), root.id, 1).await.unwrap();
        assert!(folders.list("owner".into()).await.unwrap().is_empty());
    }

    #[actix_web::test]
    async fn competing_moves_cannot_create_a_cycle_and_only_one_revision_wins() {
        let database = TestDb::new();
        database.insert_user("owner");
        let folders = FolderService {
            pool: database.pool.clone(),
        };
        let a = folders
            .create(
                "owner".into(),
                NewFolder {
                    name: "A".into(),
                    parent_id: None,
                },
            )
            .await
            .unwrap();
        let b = folders
            .create(
                "owner".into(),
                NewFolder {
                    name: "B".into(),
                    parent_id: None,
                },
            )
            .await
            .unwrap();
        let (first, second) = futures_for_moves(&folders, &a.id, &b.id).await;
        assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
        assert!(
            matches!(first, Err(Error::BadRequest(_)))
                || matches!(second, Err(Error::BadRequest(_)))
        );
        let revision = dao::owned(&mut database.pool.get().unwrap(), "owner", &a.id)
            .unwrap()
            .revision;
        let (first, second) = tokio::join!(
            folders.update(
                "owner".into(),
                a.id.clone(),
                revision,
                FolderPatch {
                    name: Some("First".into()),
                    parent_id: None
                }
            ),
            folders.update(
                "owner".into(),
                a.id.clone(),
                revision,
                FolderPatch {
                    name: Some("Second".into()),
                    parent_id: None
                }
            )
        );
        assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
        assert!(
            matches!(first, Err(Error::PreconditionFailed))
                || matches!(second, Err(Error::PreconditionFailed))
        );
    }

    async fn futures_for_moves(
        service: &FolderService,
        a: &str,
        b: &str,
    ) -> (Result<Folder, Error>, Result<Folder, Error>) {
        tokio::join!(
            service.update(
                "owner".into(),
                a.into(),
                1,
                FolderPatch {
                    name: None,
                    parent_id: Some(Some(b.into()))
                }
            ),
            service.update(
                "owner".into(),
                b.into(),
                1,
                FolderPatch {
                    name: None,
                    parent_id: Some(Some(a.into()))
                }
            )
        )
    }
}
