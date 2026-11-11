use crate::{
    db::{self, DbPool, access::FolderAccess, sharing as dao},
    error::Error,
    models::{
        folder::Folder,
        note::Note,
        sharing::{FolderView, Kind, NoteView, Permission, ShareEntry},
    },
};
use diesel::SqliteConnection;
use std::collections::HashSet;

pub fn require_edit(edit: bool) -> Result<(), Error> {
    if edit { Ok(()) } else { Err(Error::Forbidden) }
}

#[derive(Clone)]
pub struct SharingService {
    pub pool: DbPool,
}

impl Kind {
    fn require_owner(self, c: &mut SqliteConnection, viewer: &str, id: &str) -> Result<(), Error> {
        let owner = match self {
            Self::Note => db::notes::get(c, viewer, id)?.user_id,
            Self::Folder => dao::readable_folder(c, viewer, id)?.user_id,
        };
        if owner != viewer {
            return Err(Error::Forbidden);
        }
        Ok(())
    }
}

fn access(owner: &str, viewer: &str, edit: bool) -> &'static str {
    if owner == viewer {
        "owner"
    } else if edit {
        "edit"
    } else {
        "view"
    }
}

impl SharingService {
    pub async fn present_notes(
        &self,
        viewer: String,
        notes: Vec<Note>,
    ) -> Result<Vec<NoteView>, Error> {
        db::run(self.pool.clone(), move |c| {
            let folders = FolderAccess::load(c, &viewer)?;
            let shares = dao::note_shares(c, &viewer)?;
            let owners = dao::owners(c, notes.iter().map(|n| n.user_id.as_str()).collect())?;
            notes
                .into_iter()
                .map(|note| {
                    let edit = dao::effective_note_permission(&note, &viewer, &folders, &shares)
                        .ok_or(Error::NotFound)?;
                    let owner = owners.get(&note.user_id).ok_or(Error::NotFound)?.clone();
                    Ok(NoteView {
                        access: access(&note.user_id, &viewer, edit),
                        note,
                        owner,
                    })
                })
                .collect()
        })
        .await
    }

    pub async fn present_note(&self, viewer: String, note: Note) -> Result<NoteView, Error> {
        self.present_notes(viewer, vec![note])
            .await?
            .pop()
            .ok_or(Error::NotFound)
    }

    pub async fn present_folders(
        &self,
        viewer: String,
        folders: Vec<Folder>,
    ) -> Result<Vec<FolderView>, Error> {
        db::run(self.pool.clone(), move |c| {
            let permissions = FolderAccess::load(c, &viewer)?.permissions;
            let owners = dao::owners(c, folders.iter().map(|f| f.user_id.as_str()).collect())?;
            let ids: HashSet<_> = permissions.keys().cloned().collect();
            folders
                .into_iter()
                .map(|mut folder| {
                    let edit = *permissions.get(&folder.id).ok_or(Error::NotFound)?;
                    let owner = owners.get(&folder.user_id).ok_or(Error::NotFound)?.clone();
                    if folder
                        .parent_id
                        .as_ref()
                        .is_some_and(|id| !ids.contains(id))
                    {
                        folder.parent_id = None;
                    }
                    Ok(FolderView {
                        access: access(&folder.user_id, &viewer, edit),
                        folder,
                        owner,
                    })
                })
                .collect()
        })
        .await
    }

    pub async fn list(
        &self,
        viewer: String,
        kind: Kind,
        id: String,
    ) -> Result<Vec<ShareEntry>, Error> {
        db::run(self.pool.clone(), move |c| {
            kind.require_owner(c, &viewer, &id)?;
            dao::entries(c, kind, &id)
        })
        .await
    }

    pub async fn grant(
        &self,
        viewer: String,
        kind: Kind,
        id: String,
        recipient: String,
        permission: Permission,
    ) -> Result<(), Error> {
        db::run(self.pool.clone(), move |c| {
            c.immediate_transaction::<_, Error, _>(|c| {
                kind.require_owner(c, &viewer, &id)?;
                if viewer == recipient {
                    return Err(Error::BadRequest("You already own this item"));
                }
                if !db::auth::user(c, &recipient)?.verified {
                    return Err(Error::NotFound);
                }
                dao::grant(c, kind, &id, &recipient, permission)?;
                Ok(())
            })
        })
        .await
    }

    pub async fn revoke(
        &self,
        viewer: String,
        kind: Kind,
        id: String,
        recipient: String,
    ) -> Result<(), Error> {
        db::run(self.pool.clone(), move |c| {
            c.immediate_transaction::<_, Error, _>(|c| {
                kind.require_owner(c, &viewer, &id)?;
                dao::revoke(c, kind, &id, &recipient)?;
                Ok(())
            })
        })
        .await
    }
}
