pub mod senders;

use crate::error::Error;
use async_trait::async_trait;

#[derive(Clone)]
pub struct EmailMessage {
    pub from: String,
    pub destination: String,
    pub subject: String,
    pub body: String,
}

#[async_trait]
pub trait SendEmail: Send + Sync {
    async fn send(&self, message: EmailMessage) -> Result<(), Error>;
}
