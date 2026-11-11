pub mod auth;
pub mod email;
pub mod notes;

use crate::{config::Config, db::DbPool, error::Error};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppServices {
    pub auth: auth::AuthService,
    pub notes: notes::NoteService,
}

impl AppServices {
    pub async fn new(
        pool: DbPool,
        config: Arc<Config>,
        sender: Arc<dyn email::SendEmail>,
    ) -> Result<Self, Error> {
        Ok(Self {
            auth: auth::AuthService::new(pool.clone(), config, sender).await?,
            notes: notes::NoteService { pool },
        })
    }
}
