use base64::{Engine, engine::general_purpose::STANDARD};
use std::{env, io};

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub bind: String,
    pub secure_cookies: bool,
    pub signing_key: Vec<u8>,
    pub password_pepper: Vec<u8>,
    pub hash_concurrency: usize,
    pub access_token_lifetime_secs: i64,
    pub refresh_token_lifetime_secs: i64,
    pub email_from: String,
}

impl Config {
    pub fn csrf_cookie_name(&self) -> &'static str {
        if self.secure_cookies {
            "__Host-xsrf-token"
        } else {
            "xsrf-token"
        }
    }

    pub fn from_env() -> io::Result<Self> {
        dotenvy::dotenv().ok();

        let key = |name| -> io::Result<Vec<u8>> {
            let value =
                env::var(name).map_err(|_| io::Error::other(format!("{name} is required")))?;
            let bytes = STANDARD
                .decode(value)
                .map_err(|_| io::Error::other(format!("{name} must be base64")))?;
            if bytes.len() < 32 {
                return Err(io::Error::other(format!(
                    "{name} must contain at least 32 random bytes"
                )));
            }

            Ok(bytes)
        };

        let positive = |name, default: usize| -> io::Result<usize> {
            match env::var(name) {
                Ok(v) => v
                    .parse()
                    .ok()
                    .filter(|v| *v > 0)
                    .ok_or_else(|| io::Error::other(format!("{name} must be positive"))),
                Err(_) => Ok(default),
            }
        };

        let lifetime = |name, default| -> io::Result<i64> {
            i64::try_from(positive(name, default)?)
                .map_err(|_| io::Error::other(format!("{name} is too large")))
        };

        let secure_cookies = env::var("SECURE_COOKIES")
            .unwrap_or_else(|_| "true".into())
            .parse()
            .map_err(|_| io::Error::other("SECURE_COOKIES must be true or false"))?;

        let email_from = env::var("EMAIL_FROM").unwrap_or_else(|_| "notes@localhost.test".into());
        if !email_address::EmailAddress::is_valid(&email_from) {
            return Err(io::Error::other("EMAIL_FROM is invalid"));
        }

        Ok(Self {
            database_url: env::var("DATABASE_URL").unwrap_or_else(|_| "notes.sqlite".into()),
            bind: env::var("BIND").unwrap_or_else(|_| "127.0.0.1:8080".into()),
            secure_cookies,
            signing_key: key("TOKEN_SIGNING_KEY_B64")?,
            password_pepper: key("PASSWORD_PEPPER_B64")?,
            hash_concurrency: positive("HASH_CONCURRENCY", 2)?,
            access_token_lifetime_secs: lifetime("ACCESS_TOKEN_LIFETIME_SECONDS", 5 * 60)?,
            refresh_token_lifetime_secs: lifetime("REFRESH_TOKEN_LIFETIME_SECONDS", 72 * 60 * 60)?,
            email_from,
        })
    }
}
