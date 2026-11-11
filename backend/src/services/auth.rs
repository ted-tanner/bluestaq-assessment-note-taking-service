use crate::{
    config::Config,
    db::{self, DbPool, auth as dao},
    error::Error,
    models::{
        blacklisted_token::BlacklistedToken,
        email_challenge::EmailChallenge,
        user::{User, UserInfo},
    },
    services::email::{EmailMessage, SendEmail},
    utils::crypto::{self, EmailCode},
};
use diesel::SqliteConnection;
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};
use uuid::Uuid;
use zeroize::Zeroizing;

#[derive(Clone)]
pub struct AuthService {
    pub pool: DbPool,
    pub config: Arc<Config>,
    sender: Arc<dyn SendEmail>,
    hashing: Arc<Semaphore>,
    dummy_password_hash: Arc<String>,
    last_blacklist_clear_time: Arc<Mutex<i64>>,
}

#[derive(Serialize)]
pub struct ChallengeInfo {
    pub challenge_id: String,
    pub purpose: String,
    pub expires_in: i64,
}

pub struct SignInResult {
    pub user: UserInfo,
    pub access_token: String,
    pub expires_at: i64,
    pub refresh_token: String,
    pub refresh_expires_at: i64,
}

struct PendingEmail {
    info: ChallengeInfo,
    message: EmailMessage,
}

impl AuthService {
    pub async fn new(
        pool: DbPool,
        config: Arc<Config>,
        sender: Arc<dyn SendEmail>,
    ) -> Result<Self, Error> {
        let hash_config = config.clone();
        let dummy_password_hash =
            crypto::run(move || crypto::hash_password(&hash_config, &crypto::random_secret()))
                .await?;

        Ok(Self {
            pool,
            hashing: Arc::new(Semaphore::new(config.hash_concurrency)),
            config,
            sender,
            dummy_password_hash: Arc::new(dummy_password_hash),
            last_blacklist_clear_time: Arc::new(Mutex::new(crypto::now())),
        })
    }

    async fn crypto<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&Config) -> Result<T, Error> + Send + 'static,
    ) -> Result<T, Error> {
        let permit = self
            .hashing
            .clone()
            .try_acquire_owned()
            .map_err(|_| Error::RateLimited)?;
        let config = self.config.clone();

        crypto::run(move || {
            // Keep the permit until CPU work ends, even if the request is cancelled.
            let _permit = permit;
            operation(&config)
        })
        .await
    }

    async fn deliver(&self, pending: PendingEmail) -> Result<ChallengeInfo, Error> {
        if self.sender.send(pending.message).await.is_err() {
            let id = pending.info.challenge_id;

            db::run(self.pool.clone(), move |conn| {
                dao::delete_challenge(conn, &id)?;

                Ok(())
            })
            .await?;

            return Err(Error::Unavailable);
        }

        Ok(pending.info)
    }

    pub async fn register(
        &self,
        email: String,
        password: Zeroizing<String>,
    ) -> Result<ChallengeInfo, Error> {
        let email = normalize_email(&email)?;
        validate_password(&password)?;

        let password_hash = self
            .crypto(move |config| crypto::hash_password(config, &password))
            .await?;
        let code = self.crypto(|_| crypto::new_email_code()).await?;
        let config = self.config.clone();
        let pending = db::run(self.pool.clone(), move |conn| {
            conn.immediate_transaction::<_, Error, _>(|conn| {
                if dao::user_by_email(conn, &email)?.is_some() {
                    return Err(Error::Conflict);
                }

                let user = User {
                    id: Uuid::new_v4().to_string(),
                    email,
                    password_hash,
                    token_version: 0,
                    verified: false,
                    created_at: crypto::now(),
                };
                dao::insert_user(conn, &user)?;

                prepare_email(conn, &config, &user, "registration", code)
            })
        })
        .await?;

        self.deliver(pending).await
    }

    pub async fn sign_in(
        &self,
        email: String,
        password: Zeroizing<String>,
    ) -> Result<ChallengeInfo, Error> {
        let email = normalize_email(&email)?;
        if password.len() > 1024 {
            return Err(Error::Unauthorized);
        }

        let user = db::run(self.pool.clone(), move |conn| {
            Ok(dao::user_by_email(conn, &email)?)
        })
        .await?;
        let stored_hash = user
            .as_ref()
            .map(|u| u.password_hash.clone())
            .unwrap_or_else(|| self.dummy_password_hash.as_ref().clone());
        let matches = self
            .crypto(move |config| crypto::verify_password(config, &password, &stored_hash))
            .await?;
        let user = user.filter(|_| matches).ok_or(Error::Unauthorized)?;
        let code = self.crypto(|_| crypto::new_email_code()).await?;
        let config = self.config.clone();

        let pending = db::run(self.pool.clone(), move |conn| {
            conn.immediate_transaction::<_, Error, _>(|conn| {
                let current = dao::user(conn, &user.id)?;
                // Password reset may have completed while verification ran on Rayon.
                if current.password_hash != user.password_hash {
                    return Err(Error::Unauthorized);
                }

                let purpose = if current.verified {
                    "signin"
                } else {
                    "registration"
                };
                prepare_email(conn, &config, &current, purpose, code)
            })
        })
        .await?;

        self.deliver(pending).await
    }

    pub async fn resend(&self, challenge_id: String) -> Result<ChallengeInfo, Error> {
        let id = challenge_id.clone();
        db::run(self.pool.clone(), move |conn| {
            let challenge = dao::challenge(conn, &id)?.ok_or(Error::Unauthorized)?;
            if challenge.created_at + 24 * 3600 <= crypto::now() {
                return Err(Error::Unauthorized);
            }

            Ok(())
        })
        .await?;

        let code = self.crypto(|_| crypto::new_email_code()).await?;
        let config = self.config.clone();
        let pending = db::run(self.pool.clone(), move |conn| {
            conn.immediate_transaction::<_, Error, _>(|conn| {
                // Consumed/replaced challenges cannot be revived after the hash completes.
                let challenge = dao::challenge(conn, &challenge_id)?.ok_or(Error::Unauthorized)?;
                if challenge.created_at + 24 * 3600 <= crypto::now() {
                    return Err(Error::Unauthorized);
                }

                let user = dao::user(conn, &challenge.user_id)?;
                prepare_email(conn, &config, &user, &challenge.purpose, code)
            })
        })
        .await?;

        self.deliver(pending).await
    }

    async fn load_challenge(
        &self,
        id: String,
        purpose: &'static str,
    ) -> Result<EmailChallenge, Error> {
        db::run(self.pool.clone(), move |conn| {
            let challenge = dao::challenge(conn, &id)?.ok_or(Error::Unauthorized)?;
            if challenge.purpose != purpose
                || challenge.expires_at <= crypto::now()
                || challenge.attempts >= 5
            {
                return Err(Error::Unauthorized);
            }

            Ok(challenge)
        })
        .await
    }

    pub async fn verify_email(
        &self,
        id: String,
        code: Zeroizing<String>,
        purpose: &'static str,
    ) -> Result<SignInResult, Error> {
        let challenge = self.load_challenge(id, purpose).await?;
        let hash = challenge.code_hash.clone();
        let matches = self
            .crypto(move |_| crypto::verify_code(&code, &hash))
            .await?;
        let config = self.config.clone();

        db::run(self.pool.clone(), move |conn| {
            // Failed attempts commit; unexpected database errors roll back.
            conn.immediate_transaction::<_, Error, _>(|conn| {
                let user = match consume_code(conn, &challenge, matches)? {
                    Ok(user) => user,
                    Err(error) => return Ok(Err(error)),
                };

                if purpose == "registration" {
                    if user.verified {
                        return Ok(Err(Error::Unauthorized));
                    }

                    dao::activate(conn, &user.id)?;
                } else if !user.verified {
                    return Ok(Err(Error::Unauthorized));
                }

                Ok(Ok(issue_tokens(&config, user, crypto::now())?))
            })?
        })
        .await
    }

    pub async fn refresh(&self, token: String) -> Result<SignInResult, Error> {
        let claims = crypto::verify(&self.config, &token, crypto::TokenType::Refresh)?;
        let config = self.config.clone();
        let mut last_clear = self.last_blacklist_clear_time.clone().lock_owned().await;

        db::run(self.pool.clone(), move |conn| {
            let user = dao::user(conn, &claims.user_id).map_err(|error| {
                if error == diesel::result::Error::NotFound {
                    Error::Unauthorized
                } else {
                    error.into()
                }
            })?;
            if !user.verified || user.token_version != claims.token_version {
                return Err(Error::Unauthorized);
            }

            let tokens = issue_tokens(&config, user, claims.authenticated_at)?;
            let blacklist = BlacklistedToken {
                token_hash: crypto::refresh_token_hash(&config, &token),
                expires_at: claims.expiration,
            };
            if !blacklist_refresh_token(
                conn,
                &blacklist,
                config.refresh_token_lifetime_secs,
                &mut last_clear,
                crypto::now(),
                Some(&claims),
            )? {
                return Err(Error::Unauthorized);
            }

            Ok(tokens)
        })
        .await
    }

    pub async fn logout(&self, token: Option<String>) -> Result<(), Error> {
        let Some(token) = token else {
            return Ok(());
        };
        let claims = match crypto::verify(&self.config, &token, crypto::TokenType::Refresh) {
            Ok(claims) => claims,
            Err(Error::Unauthorized) => return Ok(()),
            Err(error) => return Err(error),
        };
        let config = self.config.clone();
        let mut last_clear = self.last_blacklist_clear_time.clone().lock_owned().await;

        db::run(self.pool.clone(), move |conn| {
            let blacklist = BlacklistedToken {
                token_hash: crypto::refresh_token_hash(&config, &token),
                expires_at: claims.expiration,
            };
            blacklist_refresh_token(
                conn,
                &blacklist,
                config.refresh_token_lifetime_secs,
                &mut last_clear,
                crypto::now(),
                None,
            )?;
            Ok(())
        })
        .await
    }

    pub async fn request_reset(&self, email: String) -> Result<ChallengeInfo, Error> {
        let email = normalize_email(&email)?;
        let user = db::run(self.pool.clone(), move |conn| {
            Ok(dao::user_by_email(conn, &email)?.filter(|u| u.verified))
        })
        .await?;

        let pending = if let Some(user) = user {
            let code = self.crypto(|_| crypto::new_email_code()).await?;
            let config = self.config.clone();

            db::run(self.pool.clone(), move |conn| {
                conn.immediate_transaction::<_, Error, _>(|conn| {
                    let current = dao::user(conn, &user.id)?;
                    if !current.verified {
                        return Ok(None);
                    }

                    match prepare_email(conn, &config, &current, "reset", code) {
                        Ok(pending) => Ok(Some(pending)),
                        Err(Error::RateLimited) => Ok(None),
                        Err(error) => Err(error),
                    }
                })
            })
            .await?
        } else {
            None
        };

        match pending {
            Some(pending) => self.deliver(pending).await,
            None => Ok(ChallengeInfo {
                challenge_id: Uuid::new_v4().to_string(),
                purpose: "reset".into(),
                expires_in: crypto::CODE_SECONDS,
            }),
        }
    }

    pub async fn confirm_reset(
        &self,
        id: String,
        code: Zeroizing<String>,
        password: Zeroizing<String>,
    ) -> Result<(), Error> {
        validate_password(&password)?;
        let challenge = self.load_challenge(id, "reset").await?;
        let hash = challenge.code_hash.clone();
        let matches = self
            .crypto(move |_| crypto::verify_code(&code, &hash))
            .await?;
        let new_hash = if matches {
            Some(
                self.crypto(move |config| crypto::hash_password(config, &password))
                    .await?,
            )
        } else {
            None
        };

        db::run(self.pool.clone(), move |conn| {
            conn.immediate_transaction::<_, Error, _>(|conn| {
                let user = match consume_code(conn, &challenge, new_hash.is_some())? {
                    Ok(user) => user,
                    Err(error) => return Ok(Err(error)),
                };

                if !user.verified {
                    return Ok(Err(Error::Unauthorized));
                }

                dao::update_password(conn, &user.id, &new_hash.ok_or(Error::Internal)?)?;
                dao::delete_user_challenges(conn, &user.id)?;

                Ok(Ok(()))
            })?
        })
        .await
    }
}

fn issue_tokens(config: &Config, user: User, authenticated_at: i64) -> Result<SignInResult, Error> {
    let token_version = user.token_version;
    let user = UserInfo::from(user);
    let access = crypto::issue_token(
        config,
        &user,
        token_version,
        authenticated_at,
        crypto::TokenType::Access,
    )?;
    let refresh = crypto::issue_token(
        config,
        &user,
        token_version,
        authenticated_at,
        crypto::TokenType::Refresh,
    )?;

    Ok(SignInResult {
        user,
        access_token: access.value,
        expires_at: access.expires_at,
        refresh_token: refresh.value,
        refresh_expires_at: refresh.expires_at,
    })
}

fn blacklist_refresh_token(
    conn: &mut SqliteConnection,
    token: &BlacklistedToken,
    refresh_lifetime_secs: i64,
    last_clear: &mut i64,
    now: i64,
    refresh_claims: Option<&crypto::Claims>,
) -> Result<bool, Error> {
    let clear_due = now.saturating_sub(*last_clear) > 6 * 60 * 60;
    let (inserted, cleared) = conn.immediate_transaction::<_, Error, _>(|conn| {
        if token.expires_at <= crypto::now() {
            return Ok((false, false));
        }

        if let Some(claims) = refresh_claims {
            let user = dao::user(conn, &claims.user_id)?;
            if !user.verified || user.token_version != claims.token_version {
                return Ok((false, false));
            }
        }

        if dao::blacklist_token(conn, token)? == 0 {
            return Ok((false, false));
        }

        let cutoff = now.saturating_sub(refresh_lifetime_secs);
        let should_clear = clear_due && dao::count_old_blacklisted_tokens(conn, cutoff)? > 100;
        if should_clear {
            dao::clear_old_blacklisted_tokens(conn, cutoff)?;
        }

        Ok((true, should_clear))
    })?;

    // Advance only when a deletion has actually committed.
    if cleared {
        *last_clear = now;
    }

    Ok(inserted)
}

fn prepare_email(
    conn: &mut SqliteConnection,
    config: &Config,
    user: &User,
    purpose: &str,
    code: EmailCode,
) -> Result<PendingEmail, Error> {
    let now = crypto::now();
    if dao::challenge_for(conn, &user.id, purpose)?.is_some_and(|c| c.last_sent_at + 60 > now) {
        return Err(Error::RateLimited);
    }

    let id = Uuid::new_v4().to_string();
    let challenge = EmailChallenge {
        id: id.clone(),
        user_id: user.id.clone(),
        purpose: purpose.into(),
        code_hash: code.hash,
        expires_at: now + crypto::CODE_SECONDS,
        attempts: 0,
        last_sent_at: now,
        created_at: now,
    };
    dao::replace_challenge(conn, &challenge)?;

    let subject = match purpose {
        "registration" => "Verify your email",
        "signin" => "Your sign-in code",
        _ => "Reset your password",
    };

    Ok(PendingEmail {
        info: ChallengeInfo {
            challenge_id: id,
            purpose: purpose.into(),
            expires_in: crypto::CODE_SECONDS,
        },
        message: EmailMessage {
            from: config.email_from.clone(),
            destination: user.email.clone(),
            subject: format!("Bluestaq Notes: {subject}"),
            body: format!(
                "Your {purpose} code is: {}\nIt expires in 10 minutes. If you did not request this, ignore this email.",
                code.value.as_str()
            ),
        },
    })
}

fn consume_code(
    conn: &mut SqliteConnection,
    checked: &EmailChallenge,
    matches: bool,
) -> Result<Result<User, Error>, Error> {
    let Some(current) = dao::challenge(conn, &checked.id)? else {
        return Ok(Err(Error::Unauthorized));
    };

    // Recheck after Rayon work: expiry, attempt limits, replacement, and consumption can change.
    if current.purpose != checked.purpose
        || current.code_hash != checked.code_hash
        || current.expires_at <= crypto::now()
        || current.attempts >= 5
    {
        return Ok(Err(Error::Unauthorized));
    }

    if !matches {
        dao::failed_attempt(conn, &current.id)?;
        return Ok(Err(Error::Unauthorized));
    }

    let user = dao::user(conn, &current.user_id)?;
    dao::delete_challenge(conn, &current.id)?;

    Ok(Ok(user))
}

fn normalize_email(email: &str) -> Result<String, Error> {
    let email = email.trim().to_lowercase();
    if email.len() > 254 || !email_address::EmailAddress::is_valid(&email) {
        return Err(Error::BadRequest("Invalid email address"));
    }

    Ok(email)
}

fn validate_password(password: &str) -> Result<(), Error> {
    if password.chars().count() < 12 || password.len() > 1024 {
        return Err(Error::BadRequest(
            "Password must have at least 12 characters and at most 1024 bytes",
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::token_blacklist as b;
    use diesel::{connection::SimpleConnection, prelude::*};

    fn seed(conn: &mut SqliteConnection, prefix: &str, count: usize, expires_at: i64) {
        conn.immediate_transaction::<_, Error, _>(|conn| {
            for index in 0..count {
                dao::blacklist_token(
                    conn,
                    &BlacklistedToken {
                        token_hash: format!("{prefix}-{index}"),
                        expires_at,
                    },
                )?;
            }
            Ok(())
        })
        .unwrap();
    }

    #[actix_web::test]
    async fn cleanup_counts_eligible_rows_and_requires_more_than_100() {
        let database = crate::test_utils::TestDb::new();
        let pool = &database.pool;
        let mut conn = pool.get().unwrap();
        let now = crypto::now();
        let lifetime = 72 * 3600;
        let cutoff = now - lifetime;
        let previous = now - 6 * 3600 - 1;
        let last = Mutex::new(previous);
        seed(&mut conn, "old", 100, cutoff - 1);
        seed(&mut conn, "boundary", 1, cutoff);
        seed(&mut conn, "recently-expired", 1, now - 1);
        seed(&mut conn, "live", 1, now + lifetime);

        let token = BlacklistedToken {
            token_hash: "first".into(),
            expires_at: now + lifetime,
        };
        assert!(
            blacklist_refresh_token(
                &mut conn,
                &token,
                lifetime,
                &mut *last.lock().await,
                now,
                None
            )
            .unwrap()
        );
        assert_eq!(
            dao::count_old_blacklisted_tokens(&mut conn, cutoff).unwrap(),
            100
        );
        assert_eq!(*last.lock().await, previous);

        seed(&mut conn, "extra", 1, cutoff - 1);
        let token = BlacklistedToken {
            token_hash: "second".into(),
            expires_at: now + lifetime,
        };
        assert!(
            blacklist_refresh_token(
                &mut conn,
                &token,
                lifetime,
                &mut *last.lock().await,
                now,
                None
            )
            .unwrap()
        );
        assert_eq!(
            dao::count_old_blacklisted_tokens(&mut conn, cutoff).unwrap(),
            0
        );
        assert_eq!(*last.lock().await, now);
        for retained in [
            "boundary-0",
            "recently-expired-0",
            "live-0",
            "first",
            "second",
        ] {
            assert_eq!(
                b::table
                    .find(retained)
                    .count()
                    .get_result::<i64>(&mut conn)
                    .unwrap(),
                1
            );
        }
    }

    #[actix_web::test]
    async fn cleanup_waits_strictly_more_than_six_hours_and_uses_configured_retention() {
        let database = crate::test_utils::TestDb::new();
        let pool = &database.pool;
        let mut conn = pool.get().unwrap();
        let now = crypto::now();
        let lifetime = 3600;
        let last = Arc::new(Mutex::new(now));
        let clone = last.clone();
        seed(&mut conn, "old", 101, now - lifetime - 1);

        let token = BlacklistedToken {
            token_hash: "boundary".into(),
            expires_at: now + 10 * 3600,
        };
        assert!(
            blacklist_refresh_token(
                &mut conn,
                &token,
                lifetime,
                &mut *clone.lock().await,
                now + 6 * 3600,
                None
            )
            .unwrap()
        );
        assert_eq!(*last.lock().await, now);
        assert_eq!(
            dao::count_old_blacklisted_tokens(&mut conn, now - lifetime).unwrap(),
            101
        );

        let token = BlacklistedToken {
            token_hash: "after-boundary".into(),
            expires_at: now + 10 * 3600,
        };
        let later = now + 6 * 3600 + 1;
        assert!(
            blacklist_refresh_token(
                &mut conn,
                &token,
                lifetime,
                &mut *last.lock().await,
                later,
                None
            )
            .unwrap()
        );
        assert_eq!(*clone.lock().await, later);
        assert_eq!(
            dao::count_old_blacklisted_tokens(&mut conn, later - lifetime).unwrap(),
            0
        );
        assert_eq!(b::table.count().get_result::<i64>(&mut conn).unwrap(), 2);
    }

    #[actix_web::test]
    async fn failed_cleanup_rolls_back_insertion_and_preserves_last_clear_time() {
        let database = crate::test_utils::TestDb::new();
        let pool = &database.pool;
        let mut conn = pool.get().unwrap();
        let now = crypto::now();
        let lifetime = 3600;
        let previous = now - 6 * 3600 - 1;
        let last = Mutex::new(previous);
        seed(&mut conn, "old", 101, now - lifetime - 1);
        conn.batch_execute("CREATE TRIGGER fail_cleanup BEFORE DELETE ON token_blacklist BEGIN SELECT RAISE(ABORT, 'test cleanup failure'); END;").unwrap();
        let token = BlacklistedToken {
            token_hash: "new".into(),
            expires_at: now + lifetime,
        };

        assert!(
            blacklist_refresh_token(
                &mut conn,
                &token,
                lifetime,
                &mut *last.lock().await,
                now,
                None
            )
            .is_err()
        );
        assert_eq!(*last.lock().await, previous);
        assert_eq!(
            b::table
                .find("new")
                .count()
                .get_result::<i64>(&mut conn)
                .unwrap(),
            0
        );
        assert_eq!(
            dao::count_old_blacklisted_tokens(&mut conn, now - lifetime).unwrap(),
            101
        );

        conn.batch_execute("DROP TRIGGER fail_cleanup").unwrap();
        assert!(
            blacklist_refresh_token(
                &mut conn,
                &token,
                lifetime,
                &mut *last.lock().await,
                now,
                None
            )
            .unwrap()
        );
        assert_eq!(*last.lock().await, now);
        assert!(
            !blacklist_refresh_token(
                &mut conn,
                &token,
                lifetime,
                &mut *last.lock().await,
                now,
                None
            )
            .unwrap()
        );
        assert_eq!(b::table.count().get_result::<i64>(&mut conn).unwrap(), 1);
    }
    #[actix_web::test]
    async fn stale_token_version_is_rechecked_before_blacklist_insertion() {
        let database = crate::test_utils::TestDb::new();
        let pool = &database.pool;
        let mut conn = pool.get().unwrap();
        let now = crypto::now();
        let user_id = Uuid::new_v4().to_string();
        dao::insert_user(
            &mut conn,
            &User {
                id: user_id.clone(),
                email: "version@example.test".into(),
                password_hash: "hash".into(),
                token_version: 1,
                verified: true,
                created_at: now,
            },
        )
        .unwrap();
        let mut claims = crypto::Claims {
            token_version: 0,
            user_id,
            email: "version@example.test".into(),
            authenticated_at: now,
            issued_at: now,
            expiration: now + 3600,
            token_type: crypto::TokenType::Refresh,
            nonce: crypto::random_secret(),
        };
        let token = BlacklistedToken {
            token_hash: "test-token".into(),
            expires_at: claims.expiration,
        };
        let mut last_clear = now;
        assert!(
            !blacklist_refresh_token(&mut conn, &token, 3600, &mut last_clear, now, Some(&claims))
                .unwrap()
        );
        assert_eq!(b::table.count().get_result::<i64>(&mut conn).unwrap(), 0);

        claims.token_version = 1;
        assert!(
            blacklist_refresh_token(&mut conn, &token, 3600, &mut last_clear, now, Some(&claims))
                .unwrap()
        );
        assert_eq!(b::table.count().get_result::<i64>(&mut conn).unwrap(), 1);
    }
    #[actix_web::test]
    async fn registration_normalizes_email_and_requires_verification() {
        let fixture = crate::test_utils::Fixture::new().await;
        let auth = &fixture.services.auth;
        let challenge = auth
            .register(
                " Alice@Example.test ".into(),
                crate::test_utils::PASSWORD.to_string().into(),
            )
            .await
            .unwrap();
        assert_eq!(challenge.purpose, "registration");
        let code = fixture.code().await;
        let stored = dao::user_by_email(&mut auth.pool.get().unwrap(), "alice@example.test")
            .unwrap()
            .unwrap();
        assert!(!stored.verified);
        assert_ne!(stored.password_hash, crate::test_utils::PASSWORD);
        assert!(matches!(
            auth.register(
                "alice@example.test".into(),
                crate::test_utils::PASSWORD.to_string().into()
            )
            .await,
            Err(Error::Conflict)
        ));
        assert!(matches!(
            auth.verify_email(
                challenge.challenge_id.clone(),
                code.clone().into(),
                "signin"
            )
            .await,
            Err(Error::Unauthorized)
        ));
        let result = auth
            .verify_email(
                challenge.challenge_id.clone(),
                code.to_lowercase().into(),
                "registration",
            )
            .await
            .unwrap();
        assert_eq!(result.user.email, "alice@example.test");
        assert!(
            dao::user(&mut auth.pool.get().unwrap(), &result.user.id)
                .unwrap()
                .verified
        );
        assert!(matches!(
            auth.verify_email(challenge.challenge_id, code.into(), "registration")
                .await,
            Err(Error::Unauthorized)
        ));
    }

    #[actix_web::test]
    async fn registration_rejects_invalid_input_without_creating_records() {
        let fixture = crate::test_utils::Fixture::new().await;
        let auth = &fixture.services.auth;
        for (email, password) in [
            ("invalid", crate::test_utils::PASSWORD.to_string()),
            ("user@example.test", "short".into()),
            ("user@example.test", "x".repeat(1025)),
        ] {
            assert!(matches!(
                auth.register(email.into(), password.into()).await,
                Err(Error::BadRequest(_))
            ));
        }
        assert_eq!(
            crate::db::schema::users::table
                .count()
                .get_result::<i64>(&mut auth.pool.get().unwrap())
                .unwrap(),
            0
        );
        assert!(fixture.sender.take_messages().await.is_empty());
    }

    #[actix_web::test]
    async fn signin_checks_credentials_and_creates_the_appropriate_challenge() {
        let fixture = crate::test_utils::Fixture::new().await;
        let auth = &fixture.services.auth;
        let pending = auth
            .register(
                "user@example.test".into(),
                crate::test_utils::PASSWORD.to_string().into(),
            )
            .await
            .unwrap();
        fixture.code().await;
        fixture.age_challenge(&pending.challenge_id);
        let pending = auth
            .sign_in(
                " USER@example.test ".into(),
                crate::test_utils::PASSWORD.to_string().into(),
            )
            .await
            .unwrap();
        assert_eq!(pending.purpose, "registration");
        let user = auth
            .verify_email(
                pending.challenge_id,
                fixture.code().await.into(),
                "registration",
            )
            .await
            .unwrap();
        for (email, password) in [
            ("user@example.test", "wrong".to_string()),
            ("missing@example.test", crate::test_utils::PASSWORD.into()),
            ("user@example.test", "x".repeat(1025)),
        ] {
            assert!(matches!(
                auth.sign_in(email.into(), password.into()).await,
                Err(Error::Unauthorized)
            ));
        }
        assert!(fixture.sender.take_messages().await.is_empty());
        let challenge = auth
            .sign_in(
                "user@example.test".into(),
                crate::test_utils::PASSWORD.to_string().into(),
            )
            .await
            .unwrap();
        assert_eq!(challenge.purpose, "signin");
        let signed_in = auth
            .verify_email(
                challenge.challenge_id,
                fixture.code().await.into(),
                "signin",
            )
            .await
            .unwrap();
        assert_eq!(signed_in.user.id, user.user.id);
    }

    #[actix_web::test]
    async fn challenges_enforce_attempt_limits_expiry_and_resend_replacement() {
        use crate::db::schema::email_challenges as c;
        let fixture = crate::test_utils::Fixture::new().await;
        let auth = &fixture.services.auth;
        let challenge = auth
            .register(
                "user@example.test".into(),
                crate::test_utils::PASSWORD.to_string().into(),
            )
            .await
            .unwrap();
        let code = fixture.code().await;
        assert!(matches!(
            auth.resend(challenge.challenge_id.clone()).await,
            Err(Error::RateLimited)
        ));
        for attempt in 1..=5 {
            assert!(matches!(
                auth.verify_email(
                    challenge.challenge_id.clone(),
                    "BAD".to_string().into(),
                    "registration"
                )
                .await,
                Err(Error::Unauthorized)
            ));
            assert_eq!(
                dao::challenge(&mut auth.pool.get().unwrap(), &challenge.challenge_id)
                    .unwrap()
                    .unwrap()
                    .attempts,
                attempt
            );
        }
        assert!(matches!(
            auth.verify_email(
                challenge.challenge_id.clone(),
                code.clone().into(),
                "registration"
            )
            .await,
            Err(Error::Unauthorized)
        ));
        fixture.age_challenge(&challenge.challenge_id);
        let replacement = auth.resend(challenge.challenge_id.clone()).await.unwrap();
        assert_ne!(replacement.challenge_id, challenge.challenge_id);
        let new_code = fixture.code().await;
        assert!(
            dao::challenge(&mut auth.pool.get().unwrap(), &challenge.challenge_id)
                .unwrap()
                .is_none()
        );
        assert!(matches!(
            auth.verify_email(challenge.challenge_id, code.into(), "registration")
                .await,
            Err(Error::Unauthorized)
        ));
        diesel::update(c::table.find(&replacement.challenge_id))
            .set(c::expires_at.eq(crypto::now()))
            .execute(&mut auth.pool.get().unwrap())
            .unwrap();
        assert!(matches!(
            auth.verify_email(
                replacement.challenge_id.clone(),
                new_code.into(),
                "registration"
            )
            .await,
            Err(Error::Unauthorized)
        ));
        diesel::update(c::table.find(&replacement.challenge_id))
            .set(c::created_at.eq(crypto::now() - 86400))
            .execute(&mut auth.pool.get().unwrap())
            .unwrap();
        assert!(matches!(
            auth.resend(replacement.challenge_id).await,
            Err(Error::Unauthorized)
        ));
    }

    #[actix_web::test]
    async fn refresh_rotates_atomically_and_logout_revokes_only_the_presented_token() {
        let fixture = crate::test_utils::Fixture::new().await;
        let session = fixture.register("user@example.test").await;
        let auth = &fixture.services.auth;
        let (a, b) = tokio::join!(
            auth.refresh(session.refresh_token.clone()),
            auth.refresh(session.refresh_token.clone())
        );
        let rotated = match (a, b) {
            (Ok(tokens), Err(Error::Unauthorized)) | (Err(Error::Unauthorized), Ok(tokens)) => {
                tokens
            }
            _ => panic!("exactly one refresh should succeed"),
        };
        assert_ne!(rotated.refresh_token, session.refresh_token);
        let old = crypto::verify(
            &auth.config,
            &session.refresh_token,
            crypto::TokenType::Refresh,
        )
        .unwrap();
        let new = crypto::verify(
            &auth.config,
            &rotated.refresh_token,
            crypto::TokenType::Refresh,
        )
        .unwrap();
        assert_eq!(old.authenticated_at, new.authenticated_at);
        assert!(matches!(
            auth.refresh(session.access_token.clone()).await,
            Err(Error::Unauthorized)
        ));
        auth.logout(Some(rotated.refresh_token.clone()))
            .await
            .unwrap();
        auth.logout(Some(rotated.refresh_token.clone()))
            .await
            .unwrap();
        auth.logout(None).await.unwrap();
        auth.logout(Some("invalid".into())).await.unwrap();
        assert!(matches!(
            auth.refresh(rotated.refresh_token).await,
            Err(Error::Unauthorized)
        ));
        assert!(
            crypto::verify(
                &auth.config,
                &session.access_token,
                crypto::TokenType::Access
            )
            .is_ok()
        );
        let signin = auth
            .sign_in(
                "user@example.test".into(),
                crate::test_utils::PASSWORD.to_string().into(),
            )
            .await
            .unwrap();
        let other = auth
            .verify_email(signin.challenge_id, fixture.code().await.into(), "signin")
            .await
            .unwrap();
        auth.refresh(other.refresh_token).await.unwrap();
    }

    #[actix_web::test]
    async fn reset_changes_password_revokes_refresh_tokens_and_deletes_challenges() {
        use crate::db::schema::email_challenges as c;
        let fixture = crate::test_utils::Fixture::new().await;
        let session = fixture.register("user@example.test").await;
        let auth = &fixture.services.auth;
        let signin = auth
            .sign_in(
                "user@example.test".into(),
                crate::test_utils::PASSWORD.to_string().into(),
            )
            .await
            .unwrap();
        fixture.code().await;
        let reset = auth
            .request_reset(" USER@example.test ".into())
            .await
            .unwrap();
        let code = fixture.code().await;
        let hidden = auth
            .request_reset("user@example.test".into())
            .await
            .unwrap();
        let missing = auth
            .request_reset("missing@example.test".into())
            .await
            .unwrap();
        for info in [hidden, missing] {
            assert_eq!(info.purpose, "reset");
            assert_eq!(info.expires_in, reset.expires_in);
            assert!(
                dao::challenge(&mut auth.pool.get().unwrap(), &info.challenge_id)
                    .unwrap()
                    .is_none()
            );
        }
        assert!(fixture.sender.take_messages().await.is_empty());
        let changed = "a changed test password";
        assert!(matches!(
            auth.confirm_reset(
                reset.challenge_id.clone(),
                "BAD".to_string().into(),
                changed.to_string().into()
            )
            .await,
            Err(Error::Unauthorized)
        ));
        auth.confirm_reset(
            reset.challenge_id.clone(),
            code.clone().into(),
            changed.to_string().into(),
        )
        .await
        .unwrap();
        assert!(matches!(
            auth.confirm_reset(reset.challenge_id, code.into(), changed.to_string().into())
                .await,
            Err(Error::Unauthorized)
        ));
        let stored = dao::user(&mut auth.pool.get().unwrap(), &session.user.id).unwrap();
        assert_eq!(stored.token_version, 1);
        assert!(crypto::verify_password(&auth.config, changed, &stored.password_hash).unwrap());
        assert_eq!(
            c::table
                .filter(c::user_id.eq(&session.user.id))
                .count()
                .get_result::<i64>(&mut auth.pool.get().unwrap())
                .unwrap(),
            0
        );
        assert!(
            dao::challenge(&mut auth.pool.get().unwrap(), &signin.challenge_id)
                .unwrap()
                .is_none()
        );
        assert!(matches!(
            auth.refresh(session.refresh_token).await,
            Err(Error::Unauthorized)
        ));
        assert!(matches!(
            auth.sign_in(
                "user@example.test".into(),
                crate::test_utils::PASSWORD.to_string().into()
            )
            .await,
            Err(Error::Unauthorized)
        ));
        let challenge = auth
            .sign_in("user@example.test".into(), changed.to_string().into())
            .await
            .unwrap();
        assert_eq!(challenge.purpose, "signin");
    }

    #[actix_web::test]
    async fn email_delivery_failure_removes_challenge_and_allows_retry() {
        struct FailingSender;
        #[async_trait::async_trait]
        impl SendEmail for FailingSender {
            async fn send(&self, _: EmailMessage) -> Result<(), Error> {
                Err(Error::Internal)
            }
        }
        let fixture = crate::test_utils::Fixture::new().await;
        let session = fixture.register("user@example.test").await;
        let auth = AuthService::new(
            fixture.database.pool.clone(),
            fixture.services.auth.config.clone(),
            Arc::new(FailingSender),
        )
        .await
        .unwrap();
        assert!(matches!(
            auth.register(
                "pending@example.test".into(),
                crate::test_utils::PASSWORD.to_string().into()
            )
            .await,
            Err(Error::Unavailable)
        ));
        assert!(matches!(
            auth.sign_in(
                "user@example.test".into(),
                crate::test_utils::PASSWORD.to_string().into()
            )
            .await,
            Err(Error::Unavailable)
        ));
        assert!(matches!(
            auth.request_reset("user@example.test".into()).await,
            Err(Error::Unavailable)
        ));
        assert_eq!(
            crate::db::schema::email_challenges::table
                .count()
                .get_result::<i64>(&mut auth.pool.get().unwrap())
                .unwrap(),
            0
        );
        assert!(
            dao::user(&mut auth.pool.get().unwrap(), &session.user.id)
                .unwrap()
                .verified
        );
        let pending = fixture
            .services
            .auth
            .sign_in(
                "pending@example.test".into(),
                crate::test_utils::PASSWORD.to_string().into(),
            )
            .await
            .unwrap();
        assert_eq!(pending.purpose, "registration");
        fixture.code().await;
        let retry = fixture
            .services
            .auth
            .request_reset("user@example.test".into())
            .await
            .unwrap();
        assert_eq!(retry.purpose, "reset");
        fixture.code().await;
        fixture.age_challenge(&retry.challenge_id);
        assert!(matches!(
            auth.resend(retry.challenge_id.clone()).await,
            Err(Error::Unavailable)
        ));
        assert!(
            dao::challenge(&mut auth.pool.get().unwrap(), &retry.challenge_id)
                .unwrap()
                .is_none()
        );
    }

    #[actix_web::test]
    async fn hashing_capacity_is_shared_and_recovers_when_work_finishes() {
        let fixture = crate::test_utils::Fixture::new().await;
        let mut config = (*fixture.services.auth.config).clone();
        config.hash_concurrency = 1;
        let auth = AuthService::new(
            fixture.database.pool.clone(),
            Arc::new(config),
            fixture.sender.clone(),
        )
        .await
        .unwrap();
        let other_worker = auth.clone();
        let (started, started_rx) = tokio::sync::oneshot::channel();
        let (release, release_rx) = std::sync::mpsc::channel();
        let work = actix_web::rt::spawn(async move {
            other_worker
                .crypto(move |_| {
                    started.send(()).unwrap();
                    release_rx
                        .recv_timeout(std::time::Duration::from_secs(10))
                        .unwrap();
                    Ok(())
                })
                .await
        });
        started_rx.await.unwrap();
        assert!(matches!(
            auth.sign_in(
                "missing@example.test".into(),
                crate::test_utils::PASSWORD.to_string().into()
            )
            .await,
            Err(Error::RateLimited)
        ));
        release.send(()).unwrap();
        work.await.unwrap().unwrap();
        assert_eq!(auth.crypto(|_| Ok(42)).await.unwrap(), 42);
    }
}
