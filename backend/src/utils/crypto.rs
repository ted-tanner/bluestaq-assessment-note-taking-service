use crate::{config::Config, error::Error, models::user::UserInfo};
use argon2_kdf::{Algorithm, Hasher, Secret};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD as B64};
use hmac::{Hmac, Mac};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::{
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::oneshot;
use uuid::Uuid;
use zeroize::Zeroizing;

type HmacSha256 = Hmac<Sha256>;

pub const SESSION_SECONDS: i64 = 30 * 24 * 60 * 60;
pub const CODE_SECONDS: i64 = 10 * 60;

/// Dispatch expensive cryptography to Rayon without blocking the async executor.
pub async fn run<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T, Error> + Send + 'static,
) -> Result<T, Error> {
    let (sender, receiver) = oneshot::channel();

    rayon::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation))
            .unwrap_or(Err(Error::Internal));

        // Request cancellation drops the receiver; it must not panic the Rayon worker.
        let _ = sender.send(result);
    });

    receiver.await.map_err(|_| Error::Internal)?
}

pub struct EmailCode {
    pub value: Zeroizing<String>,
    pub hash: String,
}

/// Call on the crypto pool: generating a code also performs its Argon2 hash.
pub fn new_email_code() -> Result<EmailCode, Error> {
    let value = Zeroizing::new(generate_code());
    let hash = hash_code(&value)?;

    Ok(EmailCode { value, hash })
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock is before Unix epoch")
        .as_secs() as i64
}

pub fn random_secret() -> String {
    let mut bytes = [0; 32];
    OsRng.fill_bytes(&mut bytes);
    B64.encode(bytes)
}

pub fn hash_password(config: &Config, password: &str) -> Result<String, Error> {
    Hasher::new()
        .algorithm(Algorithm::Argon2id)
        .secret(Secret::using(&config.password_pepper))
        .hash_length(32)
        .memory_cost_kib(32768)
        .iterations(4)
        .threads(1)
        .salt_length(16)
        .hash(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|_| Error::Internal)
}

fn hash_argon(value: &[u8], memory: u32, iterations: u32) -> Result<String, Error> {
    Hasher::new()
        .algorithm(Algorithm::Argon2id)
        .hash_length(32)
        .memory_cost_kib(memory)
        .iterations(iterations)
        .threads(1)
        .salt_length(16)
        .hash(value)
        .map(|h| h.to_string())
        .map_err(|_| Error::Internal)
}

pub fn verify_password(config: &Config, password: &str, stored: &str) -> Result<bool, Error> {
    let hash = argon2_kdf::Hash::from_str(stored).map_err(|_| Error::Internal)?;

    Ok(hash.verify_with_secret(password.as_bytes(), Secret::using(&config.password_pepper)))
}

fn generate_code() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut code = String::with_capacity(8);

    while code.len() < 8 {
        let mut byte = [0];
        OsRng.fill_bytes(&mut byte);
        if byte[0] < 252 {
            code.push(ALPHABET[(byte[0] % 36) as usize] as char);
        }
    }

    code
}

pub fn hash_code(code: &str) -> Result<String, Error> {
    hash_argon(code.as_bytes(), 8192, 8)
}

pub fn verify_code(code: &str, hash: &str) -> Result<bool, Error> {
    let normalized = Zeroizing::new(code.trim().to_ascii_uppercase());

    // Still do the KDF for malformed codes; callers persist all failed attempts.
    let matches = argon2_kdf::Hash::from_str(hash)
        .map_err(|_| Error::Internal)?
        .verify(normalized.as_bytes());

    Ok(normalized.len() == 8 && normalized.bytes().all(|b| b.is_ascii_alphanumeric()) && matches)
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenType {
    Access,
    Refresh,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Claims {
    pub token_version: i64,
    pub token_type: TokenType,
    pub nonce: String,
    pub user_id: String,
    pub email: String,
    pub authenticated_at: i64,
    pub issued_at: i64,
    pub expiration: i64,
}

pub struct IssuedToken {
    pub value: String,
    pub expires_at: i64,
}

pub fn issue_token(
    config: &Config,
    user: &UserInfo,
    token_version: i64,
    authenticated_at: i64,
    token_type: TokenType,
) -> Result<IssuedToken, Error> {
    let issued_at = now();
    let deadline = authenticated_at
        .checked_add(SESSION_SECONDS)
        .ok_or(Error::Unauthorized)?;
    if token_version < 0
        || authenticated_at < 0
        || authenticated_at > issued_at
        || deadline <= issued_at
    {
        return Err(Error::Unauthorized);
    }

    let lifetime = match token_type {
        TokenType::Access => config.access_token_lifetime_secs,
        TokenType::Refresh => config.refresh_token_lifetime_secs,
    };
    let expiration = issued_at
        .checked_add(lifetime)
        .ok_or(Error::Internal)?
        .min(deadline);
    let claims = Claims {
        token_version,
        token_type,
        nonce: random_secret(),
        user_id: user.id.clone(),
        email: user.email.clone(),
        authenticated_at,
        issued_at,
        expiration,
    };

    Ok(IssuedToken {
        value: sign(config, &claims)?,
        expires_at: expiration,
    })
}

pub fn sign(config: &Config, claims: &Claims) -> Result<String, Error> {
    let mut bytes = serde_json::to_vec(claims).map_err(|_| Error::Internal)?;
    let mut mac = HmacSha256::new_from_slice(&config.signing_key).map_err(|_| Error::Internal)?;
    mac.update(&bytes);
    bytes.extend_from_slice(&mac.finalize().into_bytes());

    Ok(B64.encode(bytes))
}

pub fn verify(config: &Config, token: &str, expected_type: TokenType) -> Result<Claims, Error> {
    if token.len() > 3800 {
        return Err(Error::Unauthorized);
    }

    let bytes = B64.decode(token).map_err(|_| Error::Unauthorized)?;
    if bytes.len() <= 32 {
        return Err(Error::Unauthorized);
    }

    let (json, signature) = bytes.split_at(bytes.len() - 32);

    let mut mac = HmacSha256::new_from_slice(&config.signing_key).map_err(|_| Error::Internal)?;
    mac.update(json);
    mac.verify_slice(signature)
        .map_err(|_| Error::Unauthorized)?;

    let claims: Claims = serde_json::from_slice(json).map_err(|_| Error::Unauthorized)?;
    let now = now();
    let absolute_expiration = claims
        .authenticated_at
        .checked_add(SESSION_SECONDS)
        .ok_or(Error::Unauthorized)?;
    let lifetime = match expected_type {
        TokenType::Access => config.access_token_lifetime_secs,
        TokenType::Refresh => config.refresh_token_lifetime_secs,
    };
    let maximum_expiration = claims
        .issued_at
        .checked_add(lifetime)
        .ok_or(Error::Unauthorized)?;
    let user_id = Uuid::parse_str(&claims.user_id).map_err(|_| Error::Unauthorized)?;

    if claims.token_version < 0
        || claims.token_type != expected_type
        || claims.nonce.len() != 43
        || user_id.is_nil()
        || claims.email.len() > 254
        || !email_address::EmailAddress::is_valid(&claims.email)
        || claims.authenticated_at < 0
        || claims.authenticated_at > claims.issued_at
        || claims.issued_at > now
        || claims.expiration <= now
        || claims.expiration <= claims.issued_at
        || claims.expiration > maximum_expiration
        || claims.expiration > absolute_expiration
    {
        return Err(Error::Unauthorized);
    }

    Ok(claims)
}

/// A domain-separated fingerprint keeps bearer tokens out of the blacklist table.
pub fn refresh_token_hash(config: &Config, token: &str) -> String {
    let mut mac =
        HmacSha256::new_from_slice(&config.signing_key).expect("HMAC accepts any key length");
    mac.update(b"bluestaq.refresh-blacklist.v1\0");
    mac.update(token.as_bytes());
    B64.encode(mac.finalize().into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test_utils::PASSWORD, utils::crypto};
    use std::{sync::Arc, time::Duration};
    use tokio::sync::Semaphore;

    #[actix_web::test]
    async fn dispatches_to_rayon_and_returns_errors_without_panicking_the_worker() {
        assert!(
            run(|| Ok(rayon::current_thread_index().is_some()))
                .await
                .unwrap()
        );
        assert!(matches!(
            run::<()>(|| Err(Error::Unavailable)).await,
            Err(Error::Unavailable)
        ));
        assert!(matches!(
            run::<()>(|| panic!("test crypto panic")).await,
            Err(Error::Internal)
        ));
        assert_eq!(run(|| Ok(42)).await.unwrap(), 42);
    }

    #[actix_web::test]
    async fn cancellation_does_not_release_capacity_before_cpu_work_finishes() {
        let capacity = Arc::new(Semaphore::new(1));
        let permit = capacity.clone().acquire_owned().await.unwrap();
        let (started, started_rx) = oneshot::channel();
        let (release, release_rx) = std::sync::mpsc::channel();
        let request = actix_web::rt::spawn(run(move || {
            let _permit = permit;
            let _ = started.send(());
            release_rx
                .recv_timeout(Duration::from_secs(5))
                .map_err(|_| Error::Internal)?;
            Ok(())
        }));

        started_rx.await.unwrap();
        request.abort();
        assert!(capacity.try_acquire().is_err());
        release.send(()).unwrap();

        let permit = actix_web::rt::time::timeout(Duration::from_secs(2), capacity.acquire())
            .await
            .unwrap()
            .unwrap();
        drop(permit);
        assert_eq!(run(|| Ok(42)).await.unwrap(), 42);
    }

    #[actix_web::test]
    async fn password_hashes_use_salts_and_require_the_configured_pepper() {
        let config = crate::test_utils::config();
        let a = crypto::hash_password(&config, PASSWORD).unwrap();
        let b = crypto::hash_password(&config, PASSWORD).unwrap();
        assert_ne!(a, b);
        assert!(a.starts_with("$argon2id$"));
        assert!(crypto::verify_password(&config, PASSWORD, &a).unwrap());
        assert!(!crypto::verify_password(&config, "wrong", &a).unwrap());
        let parsed = a.parse::<argon2_kdf::Hash>().unwrap();
        assert!(parsed.verify_with_secret(
            PASSWORD.as_bytes(),
            argon2_kdf::Secret::using(&config.password_pepper)
        ));
        assert!(!parsed.verify(PASSWORD.as_bytes()));
        let mut wrong_pepper = config.clone();
        wrong_pepper.password_pepper[0] ^= 1;
        assert!(!crypto::verify_password(&wrong_pepper, PASSWORD, &a).unwrap());
    }

    #[actix_web::test]
    async fn tokens_reject_tampering_and_invalid_claims() {
        let config = crate::test_utils::config();
        let now = crypto::now();
        let claims = crypto::Claims {
            token_version: 0,
            token_type: crypto::TokenType::Access,
            nonce: crypto::random_secret(),
            user_id: uuid::Uuid::new_v4().to_string(),
            email: "user@example.test".into(),
            authenticated_at: now,
            issued_at: now,
            expiration: now + config.access_token_lifetime_secs,
        };
        let token = crypto::sign(&config, &claims).unwrap();
        assert!(crypto::verify(&config, &token, crypto::TokenType::Access).is_ok());

        let mut tampered = token.into_bytes();
        tampered[3] = if tampered[3] == b'A' { b'B' } else { b'A' };
        assert!(
            crypto::verify(
                &config,
                std::str::from_utf8(&tampered).unwrap(),
                crypto::TokenType::Access
            )
            .is_err()
        );
        assert!(matches!(
            crypto::verify(&config, "invalid", crypto::TokenType::Access),
            Err(Error::Unauthorized)
        ));

        let mut invalid = claims.clone();
        invalid.token_version = -1;
        assert!(
            crypto::verify(
                &config,
                &crypto::sign(&config, &invalid).unwrap(),
                crypto::TokenType::Access
            )
            .is_err()
        );
        invalid = claims.clone();
        invalid.expiration = now;
        assert!(
            crypto::verify(
                &config,
                &crypto::sign(&config, &invalid).unwrap(),
                crypto::TokenType::Access
            )
            .is_err()
        );
        invalid = claims.clone();
        invalid.issued_at = now + 60;
        assert!(
            crypto::verify(
                &config,
                &crypto::sign(&config, &invalid).unwrap(),
                crypto::TokenType::Access
            )
            .is_err()
        );
        invalid = claims.clone();
        invalid.expiration = now + config.access_token_lifetime_secs + 1;
        assert!(
            crypto::verify(
                &config,
                &crypto::sign(&config, &invalid).unwrap(),
                crypto::TokenType::Access
            )
            .is_err()
        );
        invalid = claims.clone();
        invalid.authenticated_at = now - crypto::SESSION_SECONDS;
        assert!(
            crypto::verify(
                &config,
                &crypto::sign(&config, &invalid).unwrap(),
                crypto::TokenType::Access
            )
            .is_err()
        );
        invalid = claims.clone();
        invalid.user_id = uuid::Uuid::nil().to_string();
        assert!(
            crypto::verify(
                &config,
                &crypto::sign(&config, &invalid).unwrap(),
                crypto::TokenType::Access
            )
            .is_err()
        );
        invalid = claims.clone();
        invalid.authenticated_at = i64::MAX;
        assert!(
            crypto::verify(
                &config,
                &crypto::sign(&config, &invalid).unwrap(),
                crypto::TokenType::Access
            )
            .is_err()
        );
    }
}
