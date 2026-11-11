use super::{
    access::FolderAccess,
    schema::{folder_shares as fs, note_shares as ns, notes as n, users as u},
};
use crate::{
    error::Error,
    models::{
        folder::Folder,
        note::Note,
        sharing::{Kind, Permission, ShareEntry},
        user::UserInfo,
    },
};
use diesel::prelude::*;
use std::collections::{HashMap, HashSet};

pub fn note_permission(c: &mut SqliteConnection, viewer: &str, id: &str) -> Result<bool, Error> {
    let note = n::table.find(id).first::<Note>(c)?;
    let folders = FolderAccess::load(c, viewer)?;
    let shares = note_shares(c, viewer)?;
    effective_note_permission(&note, viewer, &folders, &shares).ok_or(Error::NotFound)
}

pub fn folder_permission(c: &mut SqliteConnection, viewer: &str, id: &str) -> Result<bool, Error> {
    FolderAccess::load(c, viewer)?
        .permissions
        .get(id)
        .copied()
        .ok_or(Error::NotFound)
}

pub fn note_shares(c: &mut SqliteConnection, viewer: &str) -> QueryResult<HashMap<String, bool>> {
    Ok(ns::table
        .filter(ns::user_id.eq(viewer))
        .select((ns::note_id, ns::permission.eq("edit")))
        .load::<(String, bool)>(c)?
        .into_iter()
        .collect())
}

pub fn effective_note_permission(
    note: &Note,
    viewer: &str,
    folders: &FolderAccess,
    shares: &HashMap<String, bool>,
) -> Option<bool> {
    if note.user_id == viewer {
        return Some(true);
    }
    let direct = shares.get(&note.id).copied();
    let inherited = note
        .folder_id
        .as_ref()
        .and_then(|id| folders.permissions.get(id))
        .copied();
    match (direct, inherited) {
        (Some(a), Some(b)) => Some(a || b),
        (a, b) => a.or(b),
    }
}

pub fn readable_folder(c: &mut SqliteConnection, viewer: &str, id: &str) -> Result<Folder, Error> {
    folder_permission(c, viewer, id)?;
    Ok(super::folders::get(c, id)?)
}

pub fn visible_folders(c: &mut SqliteConnection, viewer: &str) -> Result<Vec<Folder>, Error> {
    let permissions = FolderAccess::load(c, viewer)?.permissions;
    // Filter in Rust to avoid a bind parameter for every accessible folder.
    Ok(super::folders::list(c)?
        .into_iter()
        .filter(|folder| permissions.contains_key(&folder.id))
        .collect())
}

pub fn entries(c: &mut SqliteConnection, kind: Kind, id: &str) -> Result<Vec<ShareEntry>, Error> {
    let rows: Vec<(UserInfo, String)> = match kind {
        Kind::Note => ns::table
            .inner_join(u::table.on(u::id.eq(ns::user_id)))
            .filter(ns::note_id.eq(id))
            .order((u::name_key, u::email))
            .select((UserInfo::as_select(), ns::permission))
            .load(c)?,
        Kind::Folder => fs::table
            .inner_join(u::table.on(u::id.eq(fs::user_id)))
            .filter(fs::folder_id.eq(id))
            .order((u::name_key, u::email))
            .select((UserInfo::as_select(), fs::permission))
            .load(c)?,
    };
    Ok(rows
        .into_iter()
        .map(|(user, permission)| ShareEntry {
            user,
            permission: if permission == "edit" {
                Permission::Edit
            } else {
                Permission::View
            },
        })
        .collect())
}

pub fn owners(
    c: &mut SqliteConnection,
    ids: Vec<&str>,
) -> Result<HashMap<String, UserInfo>, Error> {
    let ids: HashSet<_> = ids.into_iter().collect();
    let rows = u::table
        .filter(u::id.eq_any(ids))
        .select(UserInfo::as_select())
        .load::<UserInfo>(c)?;
    Ok(rows.into_iter().map(|u| (u.id.clone(), u)).collect())
}

pub fn grant(
    c: &mut SqliteConnection,
    kind: Kind,
    id: &str,
    recipient: &str,
    permission: Permission,
) -> QueryResult<()> {
    match kind {
        Kind::Note => diesel::insert_into(ns::table)
            .values((
                ns::note_id.eq(id),
                ns::user_id.eq(recipient),
                ns::permission.eq(permission.value()),
            ))
            .on_conflict((ns::note_id, ns::user_id))
            .do_update()
            .set(ns::permission.eq(permission.value()))
            .execute(c)?,
        Kind::Folder => diesel::insert_into(fs::table)
            .values((
                fs::folder_id.eq(id),
                fs::user_id.eq(recipient),
                fs::permission.eq(permission.value()),
            ))
            .on_conflict((fs::folder_id, fs::user_id))
            .do_update()
            .set(fs::permission.eq(permission.value()))
            .execute(c)?,
    };
    Ok(())
}

pub fn revoke(c: &mut SqliteConnection, kind: Kind, id: &str, recipient: &str) -> QueryResult<()> {
    match kind {
        Kind::Note => diesel::delete(
            ns::table
                .filter(ns::note_id.eq(id))
                .filter(ns::user_id.eq(recipient)),
        )
        .execute(c)?,
        Kind::Folder => diesel::delete(
            fs::table
                .filter(fs::folder_id.eq(id))
                .filter(fs::user_id.eq(recipient)),
        )
        .execute(c)?,
    };
    Ok(())
}
