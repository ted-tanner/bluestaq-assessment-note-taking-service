pub mod auth;
pub mod email;
pub mod folders;
pub mod notes;
pub mod sharing;
pub mod users;

use crate::{config::Config, db::DbPool, error::Error};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppServices {
    pub auth: auth::AuthService,
    pub notes: notes::NoteService,
    pub users: users::UserService,
    pub sharing: sharing::SharingService,
    pub folders: folders::FolderService,
}

impl AppServices {
    pub async fn new(
        pool: DbPool,
        config: Arc<Config>,
        sender: Arc<dyn email::SendEmail>,
    ) -> Result<Self, Error> {
        Ok(Self {
            auth: auth::AuthService::new(pool.clone(), config, sender).await?,
            folders: folders::FolderService { pool: pool.clone() },
            users: users::UserService { pool: pool.clone() },
            sharing: sharing::SharingService { pool: pool.clone() },
            notes: notes::NoteService { pool },
        })
    }
}
