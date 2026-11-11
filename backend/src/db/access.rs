use super::schema::{folder_shares as s, folders as f};
use diesel::prelude::*;
use std::collections::{HashMap, HashSet};

/// Resolve inherited grants once using folder metadata, without recursive SQL.
pub struct FolderAccess {
    pub permissions: HashMap<String, bool>,
    children: HashMap<String, Vec<String>>,
}

impl FolderAccess {
    pub fn load(conn: &mut SqliteConnection, viewer: &str) -> QueryResult<Self> {
        let folders =
            f::table
                .select((f::id, f::parent_id, f::user_id))
                .load::<(String, Option<String>, String)>(conn)?;
        let mut grants = s::table
            .filter(s::user_id.eq(viewer))
            .select((s::folder_id, s::permission.eq("edit")))
            .load::<(String, bool)>(conn)?;
        let mut children = HashMap::<String, Vec<String>>::new();
        for (id, parent, owner) in folders {
            if owner == viewer {
                grants.push((id.clone(), true));
            }
            if let Some(parent) = parent {
                children.entry(parent).or_default().push(id);
            }
        }
        let mut permissions = HashMap::new();
        while let Some((id, edit)) = grants.pop() {
            // Revisit only when upgrading view to edit. This also bounds cycles.
            if let Some(previous) = permissions.get(&id)
                && (*previous || !edit)
            {
                continue;
            }
            permissions.insert(id.clone(), edit);
            if let Some(nested) = children.get(&id) {
                grants.extend(nested.iter().map(|child| (child.clone(), edit)));
            }
        }
        Ok(Self {
            permissions,
            children,
        })
    }

    pub fn subtree(&self, root: &str, descendants: bool) -> Vec<String> {
        let mut ids = HashSet::from([root.to_owned()]);
        let mut pending = vec![root.to_owned()];
        while descendants && let Some(id) = pending.pop() {
            if let Some(children) = self.children.get(&id) {
                for child in children {
                    if ids.insert(child.clone()) {
                        pending.push(child.clone());
                    }
                }
            }
        }
        ids.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{models::folder::Folder, test_utils::TestDb};

    #[test]
    fn overlapping_grants_keep_the_strongest_permission_and_revoke_independently() {
        let database = TestDb::new();
        database.insert_user("owner");
        database.insert_user("viewer");
        let conn = &mut database.pool.get().unwrap();
        for (id, parent) in [
            ("root", None),
            ("branch", Some("root")),
            ("leaf", Some("branch")),
            ("private", None),
        ] {
            diesel::insert_into(f::table)
                .values(Folder {
                    id: id.into(),
                    user_id: "owner".into(),
                    name: id.into(),
                    parent_id: parent.map(str::to_owned),
                    created_at: 0,
                    updated_at: 0,
                    revision: 1,
                })
                .execute(conn)
                .unwrap();
        }
        let grant = |conn: &mut SqliteConnection, id: &str, permission: &str| {
            diesel::insert_into(s::table)
                .values((
                    s::folder_id.eq(id),
                    s::user_id.eq("viewer"),
                    s::permission.eq(permission),
                ))
                .on_conflict((s::folder_id, s::user_id))
                .do_update()
                .set(s::permission.eq(permission))
                .execute(conn)
                .unwrap();
        };

        grant(conn, "root", "view");
        grant(conn, "branch", "edit");
        let access = FolderAccess::load(conn, "viewer").unwrap();
        assert_eq!(access.permissions.get("root"), Some(&false));
        assert_eq!(access.permissions.get("branch"), Some(&true));
        assert_eq!(access.permissions.get("leaf"), Some(&true));
        assert!(!access.permissions.contains_key("private"));
        assert_eq!(
            access
                .subtree("root", true)
                .into_iter()
                .collect::<HashSet<_>>(),
            HashSet::from(["root".into(), "branch".into(), "leaf".into()])
        );
        assert_eq!(access.subtree("root", false), vec!["root"]);

        // A weaker direct grant cannot downgrade inherited edit access.
        grant(conn, "root", "edit");
        grant(conn, "branch", "view");
        let access = FolderAccess::load(conn, "viewer").unwrap();
        assert!(access.permissions.values().all(|edit| *edit));

        // Revoking the ancestor leaves the independent branch grant in place.
        diesel::delete(s::table.filter(s::folder_id.eq("root")))
            .execute(conn)
            .unwrap();
        let access = FolderAccess::load(conn, "viewer").unwrap();
        assert!(!access.permissions.contains_key("root"));
        assert_eq!(access.permissions.get("branch"), Some(&false));
        assert_eq!(access.permissions.get("leaf"), Some(&false));
        let owner = FolderAccess::load(conn, "owner").unwrap();
        assert_eq!(owner.permissions.len(), 4);
        assert!(owner.permissions.values().all(|edit| *edit));
    }
}
