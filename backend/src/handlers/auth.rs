use crate::{
    error::Error,
    middleware::auth::{AuthenticatedUser, token_cookie},
    services::{AppServices, auth::SignInResult},
};
use actix_csrf::extractor::{Csrf, CsrfHeader, CsrfToken};
use actix_web::{
    HttpRequest, HttpResponse,
    cookie::{Cookie, SameSite, time::Duration},
    web,
};
use serde::Deserialize;
use serde_json::json;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
pub struct Registration {
    pub email: String,
    pub password: String,
    pub name: String,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
pub struct Credentials {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
pub struct VerifyCode {
    pub challenge_id: String,
    pub code: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChallengeId {
    pub challenge_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Email {
    pub email: String,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
pub struct Reset {
    pub challenge_id: String,
    pub code: String,
    pub new_password: String,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
pub struct PasswordChange {
    pub current_password: String,
    pub new_password: String,
}

pub async fn change_password(
    _csrf: Csrf<CsrfHeader>,
    req: HttpRequest,
    services: web::Data<AppServices>,
    body: web::Json<PasswordChange>,
) -> Result<HttpResponse, Error> {
    let token = req
        .cookie("access_token")
        .ok_or(Error::Unauthorized)?
        .value()
        .to_owned();
    let mut body = body.into_inner();
    let result = services
        .auth
        .change_password(
            token,
            Zeroizing::new(std::mem::take(&mut body.current_password)),
            Zeroizing::new(std::mem::take(&mut body.new_password)),
        )
        .await?;
    Ok(sign_in_response(&services, result))
}

fn sign_in_response(services: &AppServices, result: SignInResult) -> HttpResponse {
    HttpResponse::Ok()
        .cookie(token_cookie(
            &services.auth.config,
            "access_token",
            result.access_token,
            result.expires_at,
        ))
        .cookie(token_cookie(
            &services.auth.config,
            "refresh_token",
            result.refresh_token,
            result.refresh_expires_at,
        ))
        .json(json!({"user": result.user}))
}

pub async fn csrf(_token: CsrfToken) -> HttpResponse {
    HttpResponse::Ok()
        .json(json!({"message": "Send the CSRF cookie value in the x-xsrf-token header"}))
}

pub async fn register(
    _csrf: Csrf<CsrfHeader>,
    services: web::Data<AppServices>,
    body: web::Json<Registration>,
) -> Result<HttpResponse, Error> {
    let mut body = body.into_inner();
    let info = services
        .auth
        .register_with_name(
            std::mem::take(&mut body.email),
            Zeroizing::new(std::mem::take(&mut body.password)),
            std::mem::take(&mut body.name),
        )
        .await?;

    Ok(HttpResponse::Accepted().json(info))
}

pub async fn sign_in(
    _csrf: Csrf<CsrfHeader>,
    services: web::Data<AppServices>,
    body: web::Json<Credentials>,
) -> Result<HttpResponse, Error> {
    let mut body = body.into_inner();
    let info = services
        .auth
        .sign_in(
            std::mem::take(&mut body.email),
            Zeroizing::new(std::mem::take(&mut body.password)),
        )
        .await?;

    Ok(HttpResponse::Accepted().json(info))
}

pub async fn verify_registration(
    _csrf: Csrf<CsrfHeader>,
    services: web::Data<AppServices>,
    body: web::Json<VerifyCode>,
) -> Result<HttpResponse, Error> {
    let mut body = body.into_inner();
    let tokens = services
        .auth
        .verify_email(
            std::mem::take(&mut body.challenge_id),
            Zeroizing::new(std::mem::take(&mut body.code)),
            "registration",
        )
        .await?;

    Ok(sign_in_response(&services, tokens))
}

pub async fn verify_sign_in(
    _csrf: Csrf<CsrfHeader>,
    services: web::Data<AppServices>,
    body: web::Json<VerifyCode>,
) -> Result<HttpResponse, Error> {
    let mut body = body.into_inner();
    let tokens = services
        .auth
        .verify_email(
            std::mem::take(&mut body.challenge_id),
            Zeroizing::new(std::mem::take(&mut body.code)),
            "signin",
        )
        .await?;

    Ok(sign_in_response(&services, tokens))
}

pub async fn resend(
    _csrf: Csrf<CsrfHeader>,
    services: web::Data<AppServices>,
    body: web::Json<ChallengeId>,
) -> Result<HttpResponse, Error> {
    let info = services.auth.resend(body.into_inner().challenge_id).await?;

    Ok(HttpResponse::Accepted().json(info))
}

pub async fn reset_request(
    _csrf: Csrf<CsrfHeader>,
    services: web::Data<AppServices>,
    body: web::Json<Email>,
) -> Result<HttpResponse, Error> {
    let info = services.auth.request_reset(body.into_inner().email).await?;

    Ok(HttpResponse::Accepted().json(info))
}

pub async fn reset_confirm(
    _csrf: Csrf<CsrfHeader>,
    req: HttpRequest,
    services: web::Data<AppServices>,
    body: web::Json<Reset>,
) -> Result<HttpResponse, Error> {
    let mut body = body.into_inner();
    services
        .auth
        .confirm_reset(
            std::mem::take(&mut body.challenge_id),
            Zeroizing::new(std::mem::take(&mut body.code)),
            Zeroizing::new(std::mem::take(&mut body.new_password)),
        )
        .await?;

    services
        .auth
        .logout(req.cookie("refresh_token").map(|c| c.value().to_owned()))
        .await?;

    Ok(clear_session(&services))
}

pub async fn refresh(
    _csrf: Csrf<CsrfHeader>,
    req: HttpRequest,
    services: web::Data<AppServices>,
) -> Result<HttpResponse, Error> {
    let token = req
        .cookie("refresh_token")
        .ok_or(Error::Unauthorized)?
        .value()
        .to_owned();
    let result = services.auth.refresh(token).await?;

    Ok(sign_in_response(&services, result))
}

pub async fn logout(
    _csrf: Csrf<CsrfHeader>,
    req: HttpRequest,
    services: web::Data<AppServices>,
) -> Result<HttpResponse, Error> {
    services
        .auth
        .logout(req.cookie("refresh_token").map(|c| c.value().to_owned()))
        .await?;

    Ok(clear_session(&services))
}

fn clear_session(services: &AppServices) -> HttpResponse {
    let mut response = HttpResponse::NoContent();

    for name in [
        "access_token",
        "refresh_token",
        services.auth.config.csrf_cookie_name(),
    ] {
        response.cookie(
            Cookie::build(name, "")
                .path("/")
                .http_only(name != services.auth.config.csrf_cookie_name())
                .secure(services.auth.config.secure_cookies)
                .same_site(SameSite::Strict)
                .max_age(Duration::ZERO)
                .finish(),
        );
    }

    response.finish()
}

pub async fn me(
    user: AuthenticatedUser,
    services: web::Data<AppServices>,
) -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok().json(services.users.get(user.0.id).await?))
}

#[cfg(test)]
mod tests {
    use crate::{
        db, routes,
        services::AppServices,
        test_utils::{Fixture, Jar, PASSWORD, app, request, signup},
        utils::crypto,
    };
    use actix_web::{
        App,
        cookie::Cookie,
        http::{Method, StatusCode},
        test, web,
    };
    use diesel::prelude::*;
    use serde_json::json;
    use std::sync::Arc;

    #[actix_web::test]
    async fn registration_sets_secure_cookies_and_returns_normalized_identity() {
        let fixture = Fixture::new().await;
        let app = app!(fixture);
        let mut alice = Jar::new();
        let verified = signup!(app, alice, fixture, "Alice@example.test");
        assert_eq!(verified.body["user"]["email"], "alice@example.test");
        for cookie in &verified.cookies {
            assert_eq!(cookie.secure(), Some(true));
            assert_eq!(
                cookie.same_site(),
                Some(actix_web::cookie::SameSite::Strict)
            );
            assert_eq!(cookie.path(), Some("/"));
            assert_eq!(cookie.domain(), None);
            assert_eq!(
                cookie.http_only().unwrap_or(false),
                cookie.name() != "__Host-xsrf-token"
            );
        }
        let response = request!(app, alice, Method::GET, "/api/user/me", json!({}));
        assert_eq!(response.status, StatusCode::OK);
    }

    #[actix_web::test]
    async fn expired_access_tokens_deny_requests() {
        let fixture = Fixture::new().await;
        let app = app!(fixture);
        let mut jar = Jar::new();
        signup!(app, jar, fixture, "user@example.test");
        let mut claims = crypto::verify(
            &fixture.services.auth.config,
            &jar["access_token"],
            crypto::TokenType::Access,
        )
        .unwrap();
        claims.issued_at = crypto::now() - fixture.services.auth.config.access_token_lifetime_secs;
        claims.authenticated_at = claims.issued_at;
        claims.expiration = crypto::now();
        jar.insert(
            "access_token".into(),
            crypto::sign(&fixture.services.auth.config, &claims).unwrap(),
        );
        assert_eq!(
            request!(app, jar, Method::GET, "/api/user/me", json!({})).status,
            StatusCode::UNAUTHORIZED
        );
    }

    #[actix_web::test]
    async fn required_signin_2fa_and_logout_blacklists_refresh_only() {
        let fixture = Fixture::new().await;
        let app = app!(fixture);
        let mut jar = Jar::new();
        signup!(app, jar, fixture, "user@example.test");
        let response = request!(app, jar, Method::POST, "/api/auth/logout", json!({}));
        assert_eq!(response.status, StatusCode::NO_CONTENT);
        request!(app, jar, Method::GET, "/api/auth/csrf-token", json!({}));
        assert_eq!(
            request!(
                app,
                jar,
                Method::POST,
                "/api/auth/sign-in",
                json!({"email":"user@example.test","password":"incorrect"})
            )
            .status,
            StatusCode::UNAUTHORIZED
        );
        let start = request!(
            app,
            jar,
            Method::POST,
            "/api/auth/sign-in",
            json!({"email":"user@example.test","password":PASSWORD})
        );
        assert_eq!(start.status, StatusCode::ACCEPTED);
        assert_eq!(start.body["purpose"], "signin");
        assert_eq!(
            request!(app, jar, Method::GET, "/api/notes", json!({})).status,
            StatusCode::UNAUTHORIZED
        );
        let code = fixture.code().await;
        let body = json!({"challenge_id":start.body["challenge_id"],"code":code});
        assert_eq!(
            request!(
                app,
                jar,
                Method::POST,
                "/api/auth/register/verify",
                body.clone()
            )
            .status,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            request!(
                app,
                jar,
                Method::POST,
                "/api/auth/2fa/email/verify",
                body.clone()
            )
            .status,
            StatusCode::OK
        );
        assert_eq!(
            request!(app, jar, Method::POST, "/api/auth/2fa/email/verify", body).status,
            StatusCode::UNAUTHORIZED
        );
        assert!(jar.contains_key("refresh_token"));
        let claims = crypto::verify(
            &fixture.services.auth.config,
            &jar["access_token"],
            crypto::TokenType::Access,
        )
        .unwrap();
        assert_eq!(
            claims.expiration - claims.issued_at,
            fixture.services.auth.config.access_token_lifetime_secs
        );

        let mut saved = jar.clone();
        assert_eq!(
            request!(app, jar, Method::POST, "/api/auth/logout", json!({})).status,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            request!(app, jar, Method::GET, "/api/user/me", json!({})).status,
            StatusCode::UNAUTHORIZED
        );
        // Stateless logout clears this browser's cookie; copies remain valid.
        assert_eq!(
            request!(app, saved, Method::GET, "/api/user/me", json!({})).status,
            StatusCode::OK
        );
        assert_eq!(
            request!(app, saved, Method::POST, "/api/auth/refresh", json!({})).status,
            StatusCode::UNAUTHORIZED
        );
    }

    #[actix_web::test]
    async fn code_limits_expiration_resend_and_concurrent_consumption() {
        use crate::db::schema::email_challenges as c;
        let fixture = Fixture::new().await;
        let app = app!(fixture);
        let mut jar = Jar::new();
        request!(app, jar, Method::GET, "/api/auth/csrf-token", json!({}));
        let start = request!(
            app,
            jar,
            Method::POST,
            "/api/auth/register",
            json!({"email":"user@example.test","password":PASSWORD})
        );
        let old_id = start.body["challenge_id"].as_str().unwrap();
        let old_code = fixture.code().await;
        assert_eq!(
            request!(
                app,
                jar,
                Method::POST,
                "/api/auth/email/resend",
                json!({"challenge_id":old_id})
            )
            .status,
            StatusCode::TOO_MANY_REQUESTS
        );
        for _ in 0..5 {
            assert_eq!(
                request!(
                    app,
                    jar,
                    Method::POST,
                    "/api/auth/register/verify",
                    json!({"challenge_id":old_id,"code":"BAD"})
                )
                .status,
                StatusCode::UNAUTHORIZED
            );
        }
        assert_eq!(
            request!(
                app,
                jar,
                Method::POST,
                "/api/auth/register/verify",
                json!({"challenge_id":old_id,"code":old_code})
            )
            .status,
            StatusCode::UNAUTHORIZED
        );
        fixture.age_challenge(old_id);
        let resent = request!(
            app,
            jar,
            Method::POST,
            "/api/auth/email/resend",
            json!({"challenge_id":old_id})
        );
        assert_eq!(resent.status, StatusCode::ACCEPTED);
        let id = resent.body["challenge_id"].as_str().unwrap();
        let code = fixture.code().await;
        assert_ne!(id, old_id);
        assert_eq!(
            request!(
                app,
                jar,
                Method::POST,
                "/api/auth/register/verify",
                json!({"challenge_id":old_id,"code":old_code})
            )
            .status,
            StatusCode::UNAUTHORIZED
        );
        diesel::update(c::table.find(id))
            .set(c::expires_at.eq(crypto::now() - 1))
            .execute(&mut fixture.services.auth.pool.get().unwrap())
            .unwrap();
        assert_eq!(
            request!(
                app,
                jar,
                Method::POST,
                "/api/auth/register/verify",
                json!({"challenge_id":id,"code":code})
            )
            .status,
            StatusCode::UNAUTHORIZED
        );
        fixture.age_challenge(id);
        let resent = request!(
            app,
            jar,
            Method::POST,
            "/api/auth/email/resend",
            json!({"challenge_id":id})
        );
        let code = fixture.code().await;
        let body = json!({"challenge_id":resent.body["challenge_id"],"code":code});
        let mut req1 = test::TestRequest::post()
            .uri("/api/auth/register/verify")
            .set_json(&body);
        let mut req2 = test::TestRequest::post()
            .uri("/api/auth/register/verify")
            .set_json(&body);
        for (name, value) in &jar {
            req1 = req1.cookie(Cookie::new(name.clone(), value.clone()));
            req2 = req2.cookie(Cookie::new(name.clone(), value.clone()));
        }
        req1 = req1.insert_header(("x-xsrf-token", jar["__Host-xsrf-token"].clone()));
        req2 = req2.insert_header(("x-xsrf-token", jar["__Host-xsrf-token"].clone()));
        let (a, b) = tokio::join!(
            test::call_service(&app, req1.to_request()),
            test::call_service(&app, req2.to_request())
        );
        let mut statuses = [a.status().as_u16(), b.status().as_u16()];
        statuses.sort();
        assert_eq!(statuses, [200, 401]);
    }

    #[actix_web::test]
    async fn reset_changes_password_clears_browser_and_invalidates_challenges() {
        let fixture = Fixture::new().await;
        let app = app!(fixture);
        let mut jar = Jar::new();
        signup!(app, jar, fixture, "user@example.test");
        // Establish a second session before leaving another sign-in challenge outstanding.
        let first_signin = request!(
            app,
            jar,
            Method::POST,
            "/api/auth/sign-in",
            json!({"email":"user@example.test","password":PASSWORD})
        );
        let first_code = fixture.code().await;
        let mut second_session = jar.clone();
        assert_eq!(
            request!(
                app,
                second_session,
                Method::POST,
                "/api/auth/2fa/email/verify",
                json!({"challenge_id":first_signin.body["challenge_id"],"code":first_code})
            )
            .status,
            StatusCode::OK
        );
        let signin = request!(
            app,
            jar,
            Method::POST,
            "/api/auth/sign-in",
            json!({"email":"user@example.test","password":PASSWORD})
        );
        let signin_code = fixture.code().await;
        let reset = request!(
            app,
            jar,
            Method::POST,
            "/api/auth/password-reset/request",
            json!({"email":"user@example.test"})
        );
        let code = fixture.code().await;
        let unknown = request!(
            app,
            jar,
            Method::POST,
            "/api/auth/password-reset/request",
            json!({"email":"unknown@example.test"})
        );
        assert_eq!(reset.status, unknown.status);
        assert_eq!(reset.body["purpose"], unknown.body["purpose"]);
        assert_eq!(reset.body["expires_in"], unknown.body["expires_in"]);
        assert_eq!(
            reset.body.as_object().unwrap().len(),
            unknown.body.as_object().unwrap().len()
        );
        assert!(fixture.sender.take_messages().await.is_empty());
        let mut saved = jar.clone();
        let reset_body = json!({"challenge_id":reset.body["challenge_id"],"code":code,"new_password":"a different long password"});
        assert_eq!(
            request!(
                app,
                jar,
                Method::POST,
                "/api/auth/password-reset/confirm",
                reset_body.clone()
            )
            .status,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            request!(app, jar, Method::GET, "/api/user/me", json!({})).status,
            StatusCode::UNAUTHORIZED
        );
        // Access-token copies stay valid until expiry, but every old refresh version is invalid.
        assert_eq!(
            request!(app, saved, Method::GET, "/api/user/me", json!({})).status,
            StatusCode::OK
        );
        assert_eq!(
            request!(app, second_session, Method::GET, "/api/user/me", json!({})).status,
            StatusCode::OK
        );
        assert_eq!(
            request!(app, saved, Method::POST, "/api/auth/refresh", json!({})).status,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            request!(
                app,
                second_session,
                Method::POST,
                "/api/auth/refresh",
                json!({})
            )
            .status,
            StatusCode::UNAUTHORIZED
        );

        assert_eq!(
            request!(
                app,
                saved,
                Method::POST,
                "/api/auth/2fa/email/verify",
                json!({"challenge_id":signin.body["challenge_id"],"code":signin_code})
            )
            .status,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            request!(
                app,
                saved,
                Method::POST,
                "/api/auth/password-reset/confirm",
                reset_body
            )
            .status,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            request!(
                app,
                saved,
                Method::POST,
                "/api/auth/sign-in",
                json!({"email":"user@example.test","password":PASSWORD})
            )
            .status,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            request!(
                app,
                saved,
                Method::POST,
                "/api/auth/sign-in",
                json!({"email":"user@example.test","password":"a different long password"})
            )
            .status,
            StatusCode::ACCEPTED
        );
        assert_eq!(
            request!(
                app,
                saved,
                Method::POST,
                "/api/auth/register",
                json!({"email":"USER@example.test","password":PASSWORD})
            )
            .status,
            StatusCode::CONFLICT
        );
    }

    #[actix_web::test]
    async fn refresh_rotates_once_rejects_replay_and_accepts_expired_access() {
        let fixture = Fixture::new().await;
        let config = &fixture.services.auth.config;
        let app = app!(fixture);
        let mut jar = Jar::new();
        signup!(app, jar, fixture, "refresh@example.test");
        let old_refresh = jar["refresh_token"].clone();
        let original = crypto::verify(config, &old_refresh, crypto::TokenType::Refresh).unwrap();
        assert_eq!(original.expiration - original.issued_at, 72 * 3600);
        let mut expired_access =
            crypto::verify(config, &jar["access_token"], crypto::TokenType::Access).unwrap();
        expired_access.authenticated_at = crypto::now() - 3600;
        expired_access.issued_at = crypto::now() - 301;
        expired_access.expiration = crypto::now();
        jar.insert(
            "access_token".into(),
            crypto::sign(config, &expired_access).unwrap(),
        );
        assert_eq!(
            request!(app, jar, Method::GET, "/api/user/me", json!({})).status,
            StatusCode::UNAUTHORIZED
        );

        let refreshed = request!(app, jar, Method::POST, "/api/auth/refresh", json!({}));
        assert_eq!(refreshed.status, StatusCode::OK);
        assert_ne!(jar["refresh_token"], old_refresh);
        assert!(
            refreshed
                .cookies
                .iter()
                .all(|c| c.http_only() == Some(true))
        );
        let access =
            crypto::verify(config, &jar["access_token"], crypto::TokenType::Access).unwrap();
        let refresh =
            crypto::verify(config, &jar["refresh_token"], crypto::TokenType::Refresh).unwrap();
        assert_eq!(access.expiration - access.issued_at, 300);
        assert_eq!(refresh.authenticated_at, original.authenticated_at);
        assert_eq!(access.authenticated_at, original.authenticated_at);
        assert_eq!(
            request!(app, jar, Method::GET, "/api/user/me", json!({})).status,
            StatusCode::OK
        );

        let mut replay = jar.clone();
        replay.insert("refresh_token".into(), old_refresh);
        assert_eq!(
            request!(app, replay, Method::POST, "/api/auth/refresh", json!({})).status,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            request!(app, jar, Method::POST, "/api/auth/refresh", json!({})).status,
            StatusCode::OK
        );

        // Token types cannot be substituted in either direction.
        let mut wrong_type = jar.clone();
        wrong_type.insert("access_token".into(), jar["refresh_token"].clone());
        assert_eq!(
            request!(app, wrong_type, Method::GET, "/api/user/me", json!({})).status,
            StatusCode::UNAUTHORIZED
        );
        wrong_type = jar.clone();
        wrong_type.insert("refresh_token".into(), jar["access_token"].clone());
        assert_eq!(
            request!(
                app,
                wrong_type,
                Method::POST,
                "/api/auth/refresh",
                json!({})
            )
            .status,
            StatusCode::UNAUTHORIZED
        );
    }

    #[actix_web::test]
    async fn concurrent_refresh_only_rotates_once() {
        let fixture = Fixture::new().await;
        let app = app!(fixture);
        let mut jar = Jar::new();
        signup!(app, jar, fixture, "concurrent@example.test");
        let request = || {
            test::TestRequest::post()
                .uri("/api/auth/refresh")
                .cookie(Cookie::new("refresh_token", jar["refresh_token"].clone()))
                .cookie(Cookie::new(
                    "__Host-xsrf-token",
                    jar["__Host-xsrf-token"].clone(),
                ))
                .insert_header(("x-xsrf-token", jar["__Host-xsrf-token"].clone()))
                .to_request()
        };
        let (a, b) = tokio::join!(
            test::call_service(&app, request()),
            test::call_service(&app, request())
        );
        let mut statuses = [a.status().as_u16(), b.status().as_u16()];
        statuses.sort();
        assert_eq!(statuses, [200, 401]);

        use crate::db::schema::token_blacklist as b;
        assert_eq!(
            b::table
                .count()
                .get_result::<i64>(&mut fixture.services.auth.pool.get().unwrap())
                .unwrap(),
            1
        );
    }

    #[actix_web::test]
    async fn configured_lifetimes_and_absolute_deadline_are_enforced_without_access_db_lookup() {
        let fixture = Fixture::new().await;
        let mut config = (*fixture.services.auth.config).clone();
        config.access_token_lifetime_secs = 60;
        config.refresh_token_lifetime_secs = 3600;
        let services = AppServices::new(
            fixture.services.auth.pool.clone(),
            Arc::new(config.clone()),
            fixture.sender.clone(),
        )
        .await
        .unwrap();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(services))
                .configure(|cfg| routes::configure(cfg, &config)),
        )
        .await;
        let mut jar = Jar::new();
        signup!(app, jar, fixture, "configured@example.test");
        let access =
            crypto::verify(&config, &jar["access_token"], crypto::TokenType::Access).unwrap();
        let refresh =
            crypto::verify(&config, &jar["refresh_token"], crypto::TokenType::Refresh).unwrap();
        assert_eq!(access.expiration - access.issued_at, 60);
        assert_eq!(refresh.expiration - refresh.issued_at, 3600);

        let no_db = test::init_service(
            App::new()
                .configure(|cfg| routes::configure(cfg, &config))
                .route(
                    "/identity",
                    web::get().to(
                        |user: crate::middleware::auth::AuthenticatedUser| async move {
                            actix_web::HttpResponse::Ok().json(user.0)
                        },
                    ),
                ),
        )
        .await;
        let result = request!(no_db, jar, Method::GET, "/identity", json!({}));
        assert_eq!(result.status, StatusCode::OK);
        assert!(result.cookies.is_empty());

        let now = crypto::now();
        let user = crate::models::user::UserInfo {
            id: access.user_id,
            email: access.email,
            name: String::new(),
        };
        let authenticated_at = now - crypto::SESSION_SECONDS + 30;
        let near_deadline = crypto::issue_token(
            &config,
            &user,
            access.token_version,
            authenticated_at,
            crypto::TokenType::Refresh,
        )
        .unwrap();
        jar.insert("refresh_token".into(), near_deadline.value);
        assert_eq!(
            request!(app, jar, Method::POST, "/api/auth/refresh", json!({})).status,
            StatusCode::OK
        );
        for (name, kind) in [
            ("access_token", crypto::TokenType::Access),
            ("refresh_token", crypto::TokenType::Refresh),
        ] {
            let claims = crypto::verify(&config, &jar[name], kind).unwrap();
            assert_eq!(
                claims.expiration,
                authenticated_at + crypto::SESSION_SECONDS
            );
        }

        let mut expired = refresh;
        expired.authenticated_at = now - 3600;
        expired.issued_at = now - 3600;
        expired.expiration = now;
        jar.insert(
            "refresh_token".into(),
            crypto::sign(&config, &expired).unwrap(),
        );
        assert_eq!(
            request!(app, jar, Method::POST, "/api/auth/refresh", json!({})).status,
            StatusCode::UNAUTHORIZED
        );
    }

    #[actix_web::test]
    async fn password_change_rotates_cookies_and_revokes_other_browsers_across_instances() {
        let fixture = Fixture::new().await;
        let config = &fixture.services.auth.config;
        let app = app!(fixture);
        let mut jar = Jar::new();
        signup!(app, jar, fixture, "change@example.test");
        let mut other = jar.clone();
        let original_csrf = jar[config.csrf_cookie_name()].clone();
        let original_access = jar["access_token"].clone();
        let payload = json!({"current_password":PASSWORD,"new_password":"a new browser password"});
        let mut anonymous = jar.clone();
        anonymous.remove("access_token");
        assert_eq!(
            request!(
                app,
                anonymous,
                Method::POST,
                "/api/auth/password-change",
                payload.clone()
            )
            .status,
            StatusCode::UNAUTHORIZED
        );
        let mut missing_csrf = jar.clone();
        missing_csrf.remove(config.csrf_cookie_name());
        assert_eq!(
            request!(
                app,
                missing_csrf,
                Method::POST,
                "/api/auth/password-change",
                payload.clone()
            )
            .status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let wrong = request!(
            app,
            jar,
            Method::POST,
            "/api/auth/password-change",
            json!({"current_password":"wrong","new_password":"a new browser password"})
        );
        assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
        assert!(wrong.cookies.is_empty());
        let response = request!(app, jar, Method::POST, "/api/auth/password-change", payload);
        assert_eq!(response.status, StatusCode::OK);
        assert_eq!(response.body["user"]["email"], "change@example.test");
        assert_session_cookies(&response, config);
        assert_eq!(jar[config.csrf_cookie_name()], original_csrf);
        assert_ne!(jar["access_token"], original_access);
        assert_eq!(
            request!(app, jar, Method::GET, "/api/user/me", json!({})).status,
            StatusCode::OK
        );
        let restarted = test::init_service(
            App::new()
                .app_data(web::Data::new(
                    AppServices::new(
                        fixture.database.reopen(),
                        config.clone(),
                        fixture.sender.clone(),
                    )
                    .await
                    .unwrap(),
                ))
                .configure(|cfg| routes::configure(cfg, config)),
        )
        .await;
        assert_eq!(
            request!(
                restarted,
                other,
                Method::POST,
                "/api/auth/refresh",
                json!({})
            )
            .status,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            request!(restarted, jar, Method::POST, "/api/auth/refresh", json!({})).status,
            StatusCode::OK
        );
        assert_eq!(
            request!(
                restarted,
                jar,
                Method::POST,
                "/api/notes",
                json!({"title":"Still signed in"})
            )
            .status,
            StatusCode::CREATED
        );
    }

    #[actix_web::test]
    async fn password_reset_revokes_other_browsers_refresh_tokens_across_instances() {
        let fixture = Fixture::new().await;
        let config = &fixture.services.auth.config;
        let app = app!(fixture);
        let mut first = Jar::new();
        signup!(app, first, fixture, "versions@example.test");
        let mut other = first.clone();
        let signin = request!(
            app,
            other,
            Method::POST,
            "/api/auth/sign-in",
            json!({"email":"versions@example.test","password":PASSWORD})
        );
        let code = fixture.code().await;
        assert_eq!(
            request!(
                app,
                other,
                Method::POST,
                "/api/auth/2fa/email/verify",
                json!({"challenge_id":signin.body["challenge_id"],"code":code})
            )
            .status,
            StatusCode::OK
        );
        let stale_refresh = other["refresh_token"].clone();
        let old = crypto::verify(config, &stale_refresh, crypto::TokenType::Refresh).unwrap();
        assert_eq!(old.token_version, 0);

        let reset = request!(
            app,
            first,
            Method::POST,
            "/api/auth/password-reset/request",
            json!({"email":"versions@example.test"})
        );
        let code = fixture.code().await;
        assert_eq!(request!(app, first, Method::POST, "/api/auth/password-reset/confirm", json!({"challenge_id":reset.body["challenge_id"],"code":code,"new_password":"a changed version password"})).status, StatusCode::NO_CONTENT);

        // This other browser's token was never individually blacklisted.
        use crate::db::schema::token_blacklist as b;
        let pool = fixture.database.reopen();
        {
            let mut conn = pool.get().unwrap();
            assert_eq!(
                db::auth::user(&mut conn, &old.user_id)
                    .unwrap()
                    .token_version,
                1
            );
            assert_eq!(
                b::table
                    .find(crypto::refresh_token_hash(config, &stale_refresh))
                    .count()
                    .get_result::<i64>(&mut conn)
                    .unwrap(),
                0
            );
        }
        let services = AppServices::new(pool, config.clone(), fixture.sender.clone())
            .await
            .unwrap();
        let restarted = test::init_service(
            App::new()
                .app_data(web::Data::new(services))
                .configure(|cfg| routes::configure(cfg, config)),
        )
        .await;
        assert_eq!(
            request!(
                restarted,
                other,
                Method::POST,
                "/api/auth/refresh",
                json!({})
            )
            .status,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            request!(restarted, other, Method::GET, "/api/user/me", json!({})).status,
            StatusCode::OK
        );

        request!(
            restarted,
            first,
            Method::GET,
            "/api/auth/csrf-token",
            json!({})
        );
        let signin = request!(
            restarted,
            first,
            Method::POST,
            "/api/auth/sign-in",
            json!({"email":"versions@example.test","password":"a changed version password"})
        );
        let code = fixture.code().await;
        assert_eq!(
            request!(
                restarted,
                first,
                Method::POST,
                "/api/auth/2fa/email/verify",
                json!({"challenge_id":signin.body["challenge_id"],"code":code})
            )
            .status,
            StatusCode::OK
        );
        assert_eq!(
            crypto::verify(config, &first["refresh_token"], crypto::TokenType::Refresh)
                .unwrap()
                .token_version,
            1
        );
        assert_eq!(
            request!(
                restarted,
                first,
                Method::POST,
                "/api/auth/refresh",
                json!({})
            )
            .status,
            StatusCode::OK
        );
        assert_eq!(
            crypto::verify(config, &first["refresh_token"], crypto::TokenType::Refresh)
                .unwrap()
                .token_version,
            1
        );
    }

    fn assert_session_cookies(
        response: &crate::test_utils::Response,
        config: &crate::config::Config,
    ) {
        assert_eq!(response.status, StatusCode::OK);
        let mut names: Vec<_> = response
            .cookies
            .iter()
            .map(|cookie| cookie.name())
            .collect();
        names.sort();
        assert_eq!(names, ["access_token", "refresh_token"]);
        for (name, kind, lifetime) in [
            (
                "access_token",
                crypto::TokenType::Access,
                config.access_token_lifetime_secs,
            ),
            (
                "refresh_token",
                crypto::TokenType::Refresh,
                config.refresh_token_lifetime_secs,
            ),
        ] {
            let cookie = response
                .cookies
                .iter()
                .find(|cookie| cookie.name() == name)
                .unwrap();
            assert_eq!(cookie.path(), Some("/"));
            assert_eq!(cookie.domain(), None);
            assert_eq!(cookie.http_only(), Some(true));
            assert_eq!(cookie.secure().unwrap_or(false), config.secure_cookies);
            assert_eq!(
                cookie.same_site(),
                Some(actix_web::cookie::SameSite::Strict)
            );
            let claims = crypto::verify(config, cookie.value(), kind).unwrap();
            assert_eq!(claims.expiration - claims.issued_at, lifetime);
            // Cookie construction occurs between issuance and receipt. Bound the age
            // by those timestamps rather than assuming the clock stays in one second.
            let age = cookie.max_age().unwrap().whole_seconds();
            assert!((claims.expiration - crypto::now()..=lifetime).contains(&age));
        }
        assert!(response.body.get("access_token").is_none());
        assert!(response.body.get("refresh_token").is_none());
    }

    fn assert_cleared_cookies(
        response: &crate::test_utils::Response,
        config: &crate::config::Config,
    ) {
        assert_eq!(response.status, StatusCode::NO_CONTENT);
        assert!(response.body.is_null());
        let mut names: Vec<_> = response
            .cookies
            .iter()
            .map(|cookie| cookie.name())
            .collect();
        names.sort();
        let mut expected = vec!["access_token", "refresh_token", config.csrf_cookie_name()];
        expected.sort();
        assert_eq!(names, expected);
        for cookie in &response.cookies {
            assert!(cookie.value().is_empty());
            assert_eq!(cookie.max_age().unwrap().whole_seconds(), 0);
            assert_eq!(cookie.path(), Some("/"));
            assert_eq!(cookie.domain(), None);
            assert_eq!(cookie.secure().unwrap_or(false), config.secure_cookies);
            assert_eq!(
                cookie.same_site(),
                Some(actix_web::cookie::SameSite::Strict)
            );
            assert_eq!(
                cookie.http_only().unwrap_or(false),
                cookie.name() != config.csrf_cookie_name()
            );
        }
    }

    #[actix_web::test]
    async fn registration_signin_and_refresh_set_exact_session_cookie_contracts() {
        for secure_cookies in [true, false] {
            let mut fixture = Fixture::new().await;
            let config = Arc::new(crate::config::Config {
                secure_cookies,
                access_token_lifetime_secs: 120,
                refresh_token_lifetime_secs: 7200,
                ..(*fixture.services.auth.config).clone()
            });
            fixture.services = AppServices::new(
                fixture.database.pool.clone(),
                config.clone(),
                fixture.sender.clone(),
            )
            .await
            .unwrap();
            let app = app!(fixture);
            let mut jar = Jar::new();
            let registered = signup!(app, jar, fixture, "cookies@example.test");
            assert_session_cookies(&registered, &config);
            let refreshed = request!(app, jar, Method::POST, "/api/auth/refresh", json!({}));
            assert_session_cookies(&refreshed, &config);
            let challenge = request!(
                app,
                jar,
                Method::POST,
                "/api/auth/sign-in",
                json!({"email":"cookies@example.test", "password":PASSWORD})
            );
            assert_eq!(challenge.status, StatusCode::ACCEPTED);
            let code = fixture.code().await;
            let signed_in = request!(
                app,
                jar,
                Method::POST,
                "/api/auth/2fa/email/verify",
                json!({"challenge_id":challenge.body["challenge_id"], "code":code})
            );
            assert_session_cookies(&signed_in, &config);
        }
    }

    #[actix_web::test]
    async fn logout_and_password_reset_delete_all_cookies_with_matching_attributes() {
        for secure_cookies in [true, false] {
            let mut fixture = Fixture::new().await;
            let config = Arc::new(crate::config::Config {
                secure_cookies,
                ..(*fixture.services.auth.config).clone()
            });
            fixture.services = AppServices::new(
                fixture.database.pool.clone(),
                config.clone(),
                fixture.sender.clone(),
            )
            .await
            .unwrap();
            let app = app!(fixture);
            let mut jar = Jar::new();
            signup!(app, jar, fixture, "logout@example.test");
            let logged_out = request!(app, jar, Method::POST, "/api/auth/logout", json!({}));
            assert_cleared_cookies(&logged_out, &config);
            assert!(jar.is_empty());
            signup!(app, jar, fixture, "reset@example.test");
            let challenge = request!(
                app,
                jar,
                Method::POST,
                "/api/auth/password-reset/request",
                json!({"email":"reset@example.test"})
            );
            assert_eq!(challenge.status, StatusCode::ACCEPTED);
            let code = fixture.code().await;
            let reset = request!(
                app,
                jar,
                Method::POST,
                "/api/auth/password-reset/confirm",
                json!({"challenge_id":challenge.body["challenge_id"], "code":code, "new_password":"a different long password"})
            );
            assert_cleared_cookies(&reset, &config);
            assert!(jar.is_empty());
        }
    }

    #[actix_web::test]
    async fn refresh_without_a_cookie_returns_the_authentication_error_contract() {
        let fixture = Fixture::new().await;
        let app = app!(fixture);
        let mut jar = Jar::new();
        request!(app, jar, Method::GET, "/api/auth/csrf-token", json!({}));
        let response = request!(app, jar, Method::POST, "/api/auth/refresh", json!({}));
        assert_eq!(response.status, StatusCode::UNAUTHORIZED);
        assert_eq!(
            response.body,
            json!({"error":"Authentication required or credentials invalid"})
        );
        assert!(response.cookies.is_empty());
    }

    #[actix_web::test]
    async fn auth_payload_extractors_reject_missing_mistyped_unknown_and_malformed_fields() {
        let fixture = Fixture::new().await;
        let app = app!(fixture);
        let mut jar = Jar::new();
        request!(app, jar, Method::GET, "/api/auth/csrf-token", json!({}));
        for (path, valid, field) in [
            (
                "/api/auth/register",
                json!({"email":"user@example.test", "password":PASSWORD}),
                "email",
            ),
            (
                "/api/auth/sign-in",
                json!({"email":"user@example.test", "password":PASSWORD}),
                "password",
            ),
            (
                "/api/auth/register/verify",
                json!({"challenge_id":"id", "code":"code"}),
                "code",
            ),
            (
                "/api/auth/2fa/email/verify",
                json!({"challenge_id":"id", "code":"code"}),
                "challenge_id",
            ),
            (
                "/api/auth/email/resend",
                json!({"challenge_id":"id"}),
                "challenge_id",
            ),
            (
                "/api/auth/password-reset/request",
                json!({"email":"user@example.test"}),
                "email",
            ),
            (
                "/api/auth/password-change",
                json!({"current_password":PASSWORD, "new_password":PASSWORD}),
                "current_password",
            ),
            (
                "/api/auth/password-reset/confirm",
                json!({"challenge_id":"id", "code":"code", "new_password":PASSWORD}),
                "new_password",
            ),
        ] {
            let mut missing = valid.clone();
            missing.as_object_mut().unwrap().remove(field);
            let mut mistyped = valid.clone();
            mistyped[field] = json!(42);
            let mut unknown = valid;
            unknown["unexpected"] = json!(true);
            for body in [missing, mistyped, unknown] {
                let response = request!(app, jar, Method::POST, path, body);
                assert_eq!(response.status, StatusCode::BAD_REQUEST, "{path}");
                assert!(response.cookies.is_empty(), "{path}");
            }
            let req = crate::test_utils::request_builder(&jar, Method::POST, path, &json!({}))
                .set_payload("{");
            let response = test::call_service(&app, req.to_request()).await;
            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
        }
        assert!(fixture.sender.take_messages().await.is_empty());
        assert_eq!(
            crate::db::schema::users::table
                .count()
                .get_result::<i64>(&mut fixture.database.pool.get().unwrap())
                .unwrap(),
            0
        );
    }

    #[actix_web::test]
    async fn email_delivery_failures_return_service_unavailable_without_session_cookies() {
        struct FailingSender;
        #[async_trait::async_trait]
        impl crate::services::email::SendEmail for FailingSender {
            async fn send(
                &self,
                _: crate::services::email::EmailMessage,
            ) -> Result<(), crate::error::Error> {
                Err(crate::error::Error::Internal)
            }
        }
        let fixture = Fixture::new().await;
        fixture.register("user@example.test").await;
        let pending = fixture
            .services
            .auth
            .register("pending@example.test".into(), PASSWORD.to_string().into())
            .await
            .unwrap();
        fixture.code().await;
        fixture.age_challenge(&pending.challenge_id);
        let services = AppServices::new(
            fixture.database.pool.clone(),
            fixture.services.auth.config.clone(),
            Arc::new(FailingSender),
        )
        .await
        .unwrap();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(services))
                .configure(|cfg| routes::configure(cfg, &fixture.services.auth.config)),
        )
        .await;
        let mut jar = Jar::new();
        request!(app, jar, Method::GET, "/api/auth/csrf-token", json!({}));
        for (path, body) in [
            (
                "/api/auth/register",
                json!({"email":"new@example.test", "password":PASSWORD}),
            ),
            (
                "/api/auth/sign-in",
                json!({"email":"user@example.test", "password":PASSWORD}),
            ),
            (
                "/api/auth/password-reset/request",
                json!({"email":"user@example.test"}),
            ),
            (
                "/api/auth/email/resend",
                json!({"challenge_id":pending.challenge_id}),
            ),
        ] {
            let response = request!(app, jar, Method::POST, path, body);
            assert_eq!(response.status, StatusCode::SERVICE_UNAVAILABLE, "{path}");
            assert_eq!(
                response.body,
                json!({"error":"Service temporarily unavailable"}),
                "{path}"
            );
            assert!(response.cookies.is_empty(), "{path}");
        }
        assert_eq!(
            crate::db::schema::email_challenges::table
                .count()
                .get_result::<i64>(&mut fixture.database.pool.get().unwrap())
                .unwrap(),
            0
        );
    }
}
