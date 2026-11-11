mod auth;
pub mod folders;
mod health;
mod notes;
mod user;

use crate::config::Config;
use actix_csrf::{CsrfMiddleware, extractor::CsrfHeaderConfig};
use actix_web::{
    http::{Method, header::HeaderName},
    web,
};
use rand::rngs::StdRng;

pub fn configure(cfg: &mut web::ServiceConfig, config: &Config) {
    let csrf = CsrfMiddleware::<StdRng>::new()
        .cookie_name(config.csrf_cookie_name())
        .secure(config.secure_cookies)
        .http_only(false)
        .set_cookie(Method::GET, "/api/auth/csrf-token");

    cfg.app_data(web::Data::new(config.clone()))
        .app_data(web::JsonConfig::default().limit(4 * 1024 * 1024))
        .app_data(csrf.cookie_config())
        .app_data(CsrfHeaderConfig::new(HeaderName::from_static(
            "x-xsrf-token",
        )))
        .service(
            web::scope("/api")
                .wrap(csrf)
                .configure(auth::configure)
                .configure(health::configure)
                .configure(notes::configure)
                .configure(folders::configure)
                .configure(crate::handlers::sharing::configure)
                .configure(user::configure),
        );
}

#[cfg(test)]
mod tests {
    use crate::{
        routes,
        services::AppServices,
        test_utils::{Fixture, Jar, PASSWORD, app, request},
    };
    use actix_web::{
        App,
        cookie::Cookie,
        http::{Method, StatusCode},
        test, web,
    };
    use serde_json::{Value, json};
    use std::sync::Arc;

    #[actix_web::test]
    async fn csrf_validation_and_json_errors() {
        let fixture = Fixture::new().await;
        let app = app!(fixture);
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/auth/logout")
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let mut jar = Jar::new();
        request!(app, jar, Method::GET, "/api/auth/csrf-token", json!({}));
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/auth/logout")
                .cookie(Cookie::new(
                    "__Host-xsrf-token",
                    jar["__Host-xsrf-token"].clone(),
                ))
                .insert_header(("x-xsrf-token", "wrong"))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/auth/logout")
                .insert_header(("x-xsrf-token", jar["__Host-xsrf-token"].clone()))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

        jar.remove("__Host-xsrf-token");
        assert_eq!(
            request!(app, jar, Method::POST, "/api/auth/logout", json!({})).status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        request!(app, jar, Method::GET, "/api/auth/csrf-token", json!({}));
        assert_eq!(
            request!(
                app,
                jar,
                Method::POST,
                "/api/auth/register",
                json!({"unexpected":1})
            )
            .status,
            StatusCode::BAD_REQUEST
        );
    }

    #[actix_web::test]
    async fn csrf_works_across_instances_and_on_local_http() {
        let fixture = Fixture::new().await;
        let app = app!(fixture);
        let mut jar = Jar::new();
        let token = request!(app, jar, Method::GET, "/api/auth/csrf-token", json!({}));
        let cookie = &token.cookies[0];

        assert_eq!(cookie.name(), "__Host-xsrf-token");
        assert_eq!(cookie.secure(), Some(true));
        assert!(!cookie.http_only().unwrap_or(false));
        assert_eq!(
            cookie.same_site(),
            Some(actix_web::cookie::SameSite::Strict)
        );
        assert_eq!(cookie.path(), Some("/"));
        assert_eq!(cookie.domain(), None);

        // A separately initialized instance with unrelated signing keys accepts the pair.
        let other = Fixture::new().await;
        let mut config = (*other.services.auth.config).clone();
        config.signing_key = vec![9; 64];
        let second_services = AppServices::new(
            other.services.auth.pool.clone(),
            Arc::new(config.clone()),
            other.sender.clone(),
        )
        .await
        .unwrap();
        let second = test::init_service(
            App::new()
                .app_data(web::Data::new(second_services))
                .configure(|cfg| routes::configure(cfg, &config)),
        )
        .await;
        assert_eq!(
            request!(second, jar, Method::POST, "/api/auth/logout", json!({})).status,
            StatusCode::NO_CONTENT
        );

        config.secure_cookies = false;
        let local_services = AppServices::new(
            other.services.auth.pool.clone(),
            Arc::new(config.clone()),
            other.sender.clone(),
        )
        .await
        .unwrap();
        let local = test::init_service(
            App::new()
                .app_data(web::Data::new(local_services))
                .configure(|cfg| routes::configure(cfg, &config)),
        )
        .await;
        let mut local_jar = Jar::new();
        let token = request!(
            local,
            local_jar,
            Method::GET,
            "/api/auth/csrf-token",
            json!({})
        );
        let cookie = &token.cookies[0];
        assert_eq!(cookie.name(), "xsrf-token");
        assert!(!cookie.secure().unwrap_or(false));

        let response = test::call_service(
            &local,
            test::TestRequest::post()
                .uri("/api/auth/logout")
                .cookie(Cookie::new("xsrf-token", local_jar["xsrf-token"].clone()))
                .insert_header(("x-xsrf-token", local_jar["xsrf-token"].clone()))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[actix_web::test]
    async fn every_write_handler_requires_csrf() {
        let fixture = Fixture::new().await;
        let app = app!(fixture);
        let note = format!("/api/notes/{}", uuid::Uuid::new_v4());
        let folder = format!("/api/folders/{}", uuid::Uuid::new_v4());
        let note_share = format!("{note}/shares/{}", uuid::Uuid::new_v4());
        let folder_share = format!("{folder}/shares/{}", uuid::Uuid::new_v4());

        for (method, path) in [
            (Method::POST, "/api/auth/register"),
            (Method::POST, "/api/auth/register/verify"),
            (Method::POST, "/api/auth/sign-in"),
            (Method::POST, "/api/auth/2fa/email/verify"),
            (Method::POST, "/api/auth/email/resend"),
            (Method::POST, "/api/auth/refresh"),
            (Method::POST, "/api/auth/logout"),
            (Method::POST, "/api/auth/password-change"),
            (Method::POST, "/api/auth/password-reset/request"),
            (Method::POST, "/api/auth/password-reset/confirm"),
            (Method::POST, "/api/notes"),
            (Method::PATCH, note.as_str()),
            (Method::DELETE, note.as_str()),
            (Method::POST, "/api/folders"),
            (Method::PATCH, "/api/user/me"),
            (Method::PUT, note_share.as_str()),
            (Method::DELETE, note_share.as_str()),
            (Method::PUT, folder_share.as_str()),
            (Method::DELETE, folder_share.as_str()),
            (Method::PATCH, folder.as_str()),
            (Method::DELETE, folder.as_str()),
        ] {
            let response = test::call_service(
                &app,
                test::TestRequest::default()
                    .method(method)
                    .uri(path)
                    .set_json(json!({}))
                    .to_request(),
            )
            .await;
            assert_eq!(
                response.status(),
                StatusCode::UNPROCESSABLE_ENTITY,
                "{path}"
            );
        }
    }

    #[actix_web::test]
    async fn json_limit_allows_four_mib_and_rejects_larger_requests() {
        let fixture = Fixture::new().await;
        let app = app!(fixture);
        let mut jar = Jar::new();
        request!(app, jar, Method::GET, "/api/auth/csrf-token", json!({}));
        let mut payload =
            serde_json::to_string(&json!({"email":"", "password":PASSWORD, "name":"Test user"}))
                .unwrap();
        let empty_email = payload.find("\"email\":\"\"").unwrap() + "\"email\":\"".len();
        payload.insert_str(empty_email, &"x".repeat(4 * 1024 * 1024 - payload.len()));
        assert_eq!(payload.len(), 4 * 1024 * 1024);

        for (body, expected) in [
            (payload.clone(), StatusCode::BAD_REQUEST),
            (format!("{payload} "), StatusCode::PAYLOAD_TOO_LARGE),
        ] {
            let response = test::call_service(
                &app,
                test::TestRequest::post()
                    .uri("/api/auth/register")
                    .cookie(Cookie::new(
                        "__Host-xsrf-token",
                        jar["__Host-xsrf-token"].clone(),
                    ))
                    .insert_header(("x-xsrf-token", jar["__Host-xsrf-token"].clone()))
                    .insert_header(("content-type", "application/json"))
                    .set_payload(body)
                    .to_request(),
            )
            .await;
            assert_eq!(response.status(), expected);
            if expected == StatusCode::BAD_REQUEST {
                // Exactly 4 MiB reached application validation instead of the size guard.
                let body: Value = test::read_body_json(response).await;
                assert_eq!(body["error"], "Invalid email address");
            }
        }
    }
}
