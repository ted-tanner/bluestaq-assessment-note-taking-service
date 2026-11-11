use super::{folder::Folder, note::Note, user::UserInfo};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct NoteView {
    #[serde(flatten)]
    pub note: Note,
    pub access: &'static str,
    pub owner: UserInfo,
}

#[derive(Serialize)]
pub struct FolderView {
    #[serde(flatten)]
    pub folder: Folder,
    pub access: &'static str,
    pub owner: UserInfo,
}

#[derive(Clone, Copy)]
pub enum Kind {
    Note,
    Folder,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    View,
    Edit,
}

impl Permission {
    pub fn value(self) -> &'static str {
        match self {
            Self::View => "view",
            Self::Edit => "edit",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShareInput {
    pub permission: Permission,
}

#[derive(Serialize)]
pub struct ShareEntry {
    pub user: UserInfo,
    pub permission: Permission,
}
