use crate::{config::Config, error::Error, models::user::UserInfo, utils::crypto};
use actix_web::{
    FromRequest, HttpRequest,
    cookie::{Cookie, SameSite, time::Duration},
    dev::Payload,
    web,
};
use std::future::{Ready, ready};

pub struct AuthenticatedUser(pub UserInfo);

impl FromRequest for AuthenticatedUser {
    type Error = Error;
    type Future = Ready<Result<Self, Error>>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        ready((|| {
            let config = req.app_data::<web::Data<Config>>().ok_or(Error::Internal)?;
            let token = req.cookie("access_token").ok_or(Error::Unauthorized)?;
            let claims = crypto::verify(config, token.value(), crypto::TokenType::Access)?;

            Ok(Self(UserInfo {
                id: claims.user_id,
                email: claims.email,
                name: String::new(),
            }))
        })())
    }
}

pub fn token_cookie(
    config: &Config,
    name: &'static str,
    value: String,
    expires_at: i64,
) -> Cookie<'static> {
    Cookie::build(name, value)
        .path("/")
        .http_only(true)
        .secure(config.secure_cookies)
        .same_site(SameSite::Strict)
        .max_age(Duration::seconds((expires_at - crypto::now()).max(0)))
        .finish()
}
