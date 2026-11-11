use actix_web::{HttpResponse, ResponseError, http::StatusCode};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    BadRequest(&'static str),
    #[error("Authentication required or credentials invalid")]
    Unauthorized,
    #[error("You do not have permission to do that")]
    Forbidden,
    #[error("Not found")]
    NotFound,
    #[error("Account already exists; sign in or reset your password")]
    Conflict,
    #[error("Too many attempts; try again later")]
    RateLimited,
    #[error("A revision is required in If-Match")]
    PreconditionRequired,
    #[error("This item has changed; reload before saving")]
    PreconditionFailed,
    #[error("Internal server error")]
    Internal,
    #[error("Service temporarily unavailable")]
    Unavailable,
}

impl From<diesel::result::Error> for Error {
    fn from(error: diesel::result::Error) -> Self {
        match error {
            diesel::result::Error::NotFound => Self::NotFound,
            _ => {
                // Query errors can contain user data; keep it out of logs.
                log::error!("Database operation failed");
                Self::Internal
            }
        }
    }
}

impl ResponseError for Error {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::Conflict => StatusCode::CONFLICT,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::PreconditionRequired => StatusCode::PRECONDITION_REQUIRED,
            Self::PreconditionFailed => StatusCode::PRECONDITION_FAILED,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(json!({"error": self.to_string()}))
    }
}
