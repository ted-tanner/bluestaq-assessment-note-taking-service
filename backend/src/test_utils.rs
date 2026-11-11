use crate::{
    config::Config,
    db::{self, DbPool},
    services::{AppServices, auth::SignInResult, email::senders::MockSender},
    utils::crypto,
};
use actix_web::{
    body::MessageBody,
    cookie::Cookie,
    dev::ServiceResponse,
    http::{Method, StatusCode},
    test,
};
use diesel::prelude::*;
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};

pub const PASSWORD: &str = "a long test password";

pub fn config() -> Config {
    Config {
        database_url: String::new(),
        bind: "127.0.0.1:0".into(),
        secure_cookies: true,
        signing_key: vec![7; 64],
        password_pepper: vec![8; 32],
        hash_concurrency: 4,
        access_token_lifetime_secs: 300,
        refresh_token_lifetime_secs: 72 * 3600,
        email_from: "notes@example.test".into(),
    }
}

pub struct TestDb {
    pub pool: DbPool,
    // Fields drop in declaration order: close the pool before deleting its files.
    dir: tempfile::TempDir,
}

impl TestDb {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let pool = db::connect(dir.path().join("test.sqlite").to_str().unwrap()).unwrap();
        Self { pool, dir }
    }

    pub fn url(&self) -> String {
        self.dir.path().join("test.sqlite").to_str().unwrap().into()
    }

    pub fn reopen(&self) -> DbPool {
        db::connect(&self.url()).unwrap()
    }

    pub fn insert_user(&self, id: &str) {
        db::auth::insert_user(
            &mut self.pool.get().unwrap(),
            &crate::models::user::User {
                id: id.into(),
                email: format!("{id}@example.test"),
                password_hash: "unused".into(),
                token_version: 0,
                verified: true,
                created_at: crypto::now(),
            },
        )
        .unwrap();
    }
}

pub struct Fixture {
    pub services: AppServices,
    pub sender: Arc<MockSender>,
    // Keep the database directory alive until services release their pool clones.
    pub database: TestDb,
}

impl Fixture {
    pub async fn new() -> Self {
        let database = TestDb::new();
        let config = Arc::new(Config {
            database_url: database.url(),
            ..config()
        });
        let sender = Arc::new(MockSender::recording());
        let services = AppServices::new(database.pool.clone(), config, sender.clone())
            .await
            .unwrap();
        Self {
            services,
            sender,
            database,
        }
    }

    pub async fn code(&self) -> String {
        let messages = self.sender.take_messages().await;
        assert_eq!(messages.len(), 1);
        messages[0]
            .body
            .lines()
            .next()
            .unwrap()
            .rsplit_once(": ")
            .unwrap()
            .1
            .into()
    }

    pub fn age_challenge(&self, id: &str) {
        use crate::db::schema::email_challenges as c;
        diesel::update(c::table.find(id))
            .set(c::last_sent_at.eq(crypto::now() - 61))
            .execute(&mut self.database.pool.get().unwrap())
            .unwrap();
    }

    pub async fn register(&self, email: &str) -> SignInResult {
        let challenge = self
            .services
            .auth
            .register(email.into(), PASSWORD.to_string().into())
            .await
            .unwrap();
        self.services
            .auth
            .verify_email(
                challenge.challenge_id,
                self.code().await.into(),
                "registration",
            )
            .await
            .unwrap()
    }
}

pub type Jar = HashMap<String, String>;
pub struct Response {
    pub status: StatusCode,
    pub body: Value,
    pub cookies: Vec<Cookie<'static>>,
}

pub fn request_builder(jar: &Jar, method: Method, path: &str, body: &Value) -> test::TestRequest {
    let mut req = test::TestRequest::default()
        .method(method)
        .uri(path)
        .set_json(body);
    for (name, value) in jar {
        req = req.cookie(Cookie::new(name.clone(), value.clone()));
    }
    if let Some(csrf) = jar
        .get("__Host-xsrf-token")
        .or_else(|| jar.get("xsrf-token"))
    {
        req = req.insert_header(("x-xsrf-token", csrf.clone()));
    }
    req
}

pub async fn read_response<B: MessageBody>(response: ServiceResponse<B>, jar: &mut Jar) -> Response
where
    B::Error: std::fmt::Debug,
{
    let status = response.status();
    let cookies: Vec<_> = response
        .response()
        .cookies()
        .map(|c| c.into_owned())
        .collect();
    for cookie in &cookies {
        if cookie.value().is_empty() {
            jar.remove(cookie.name());
        } else {
            jar.insert(cookie.name().to_owned(), cookie.value().to_owned());
        }
    }
    let is_json = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/json"));
    let bytes = test::read_body(response).await;
    let body = if bytes.is_empty() {
        Value::Null
    } else if is_json {
        serde_json::from_slice(&bytes).unwrap()
    } else {
        Value::String(String::from_utf8(bytes.to_vec()).unwrap())
    };
    Response {
        status,
        body,
        cookies,
    }
}

// Keep Actix's opaque application/request types inferred at the call site.
macro_rules! request {
    ($app:expr, $jar:expr, $method:expr, $path:expr, $body:expr) => {{
        let req = $crate::test_utils::request_builder(&$jar, $method, $path, &$body);
        let response = actix_web::test::call_service(&$app, req.to_request()).await;
        $crate::test_utils::read_response(response, &mut $jar).await
    }};
}
macro_rules! app {
    ($fixture:expr) => {
        actix_web::test::init_service(
            actix_web::App::new()
                .app_data(actix_web::web::Data::new($fixture.services.clone()))
                .configure(|cfg| $crate::routes::configure(cfg, &$fixture.services.auth.config)),
        )
        .await
    };
}
macro_rules! signup {
    ($app:expr, $jar:expr, $fixture:expr, $email:expr) => {{
        let _ = $crate::test_utils::request!($app, $jar, actix_web::http::Method::GET, "/api/auth/csrf-token", serde_json::json!({}));
        let start = $crate::test_utils::request!($app, $jar, actix_web::http::Method::POST, "/api/auth/register", serde_json::json!({"email": $email, "password": $crate::test_utils::PASSWORD}));
        assert_eq!(start.status, actix_web::http::StatusCode::ACCEPTED, "{}", start.body);
        assert!(start.body.get("code").is_none());
        let code = $fixture.code().await;
        let verified = $crate::test_utils::request!($app, $jar, actix_web::http::Method::POST, "/api/auth/register/verify", serde_json::json!({"challenge_id": start.body["challenge_id"], "code": code}));
        assert_eq!(verified.status, actix_web::http::StatusCode::OK, "{}", verified.body);
        verified
    }};
}

pub(crate) use {app, request, signup};
