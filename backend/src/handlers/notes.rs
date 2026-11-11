use crate::{
    error::Error,
    middleware::auth::AuthenticatedUser,
    services::{
        AppServices,
        notes::{NewNote, NotePatch, Pagination},
    },
};
use actix_csrf::extractor::{Csrf, CsrfHeader};
use actix_web::{HttpResponse, web};
use uuid::Uuid;

pub async fn create(
    _csrf: Csrf<CsrfHeader>,
    user: AuthenticatedUser,
    services: web::Data<AppServices>,
    input: web::Json<NewNote>,
) -> Result<HttpResponse, Error> {
    let note = services.notes.create(user.0.id, input.into_inner()).await?;

    Ok(HttpResponse::Created()
        .append_header(("Location", format!("/api/notes/{}", note.id)))
        .json(note))
}

pub async fn list(
    user: AuthenticatedUser,
    services: web::Data<AppServices>,
    page: web::Query<Pagination>,
) -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok().json(services.notes.list(user.0.id, page.into_inner()).await?))
}

pub async fn get(
    user: AuthenticatedUser,
    services: web::Data<AppServices>,
    id: web::Path<Uuid>,
) -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok().json(services.notes.get(user.0.id, id.to_string()).await?))
}

pub async fn update(
    _csrf: Csrf<CsrfHeader>,
    user: AuthenticatedUser,
    services: web::Data<AppServices>,
    id: web::Path<Uuid>,
    input: web::Json<NotePatch>,
) -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok().json(
        services
            .notes
            .update(user.0.id, id.to_string(), input.into_inner())
            .await?,
    ))
}

pub async fn delete(
    _csrf: Csrf<CsrfHeader>,
    user: AuthenticatedUser,
    services: web::Data<AppServices>,
    id: web::Path<Uuid>,
) -> Result<HttpResponse, Error> {
    services.notes.delete(user.0.id, id.to_string()).await?;

    Ok(HttpResponse::NoContent().finish())
}

#[cfg(test)]
mod tests {
    use crate::test_utils::{Fixture, Jar, app, request, signup};
    use actix_web::http::{Method, StatusCode};
    use serde_json::json;

    #[actix_web::test]
    async fn crud_returns_owned_content_and_supports_pagination() {
        let fixture = Fixture::new().await;
        let app = app!(fixture);
        let mut alice = Jar::new();
        signup!(app, alice, fixture, "alice@example.test");
        let response = request!(
            app,
            alice,
            Method::POST,
            "/api/notes",
            json!({"title": "First", "body": "hello"})
        );
        assert_eq!(response.status, StatusCode::CREATED);
        let id = response.body["id"].as_str().unwrap().to_owned();
        assert!(response.body.get("user_id").is_none());
        let response = request!(
            app,
            alice,
            Method::PATCH,
            &format!("/api/notes/{id}"),
            json!({"body": "updated"})
        );
        assert_eq!(response.body["title"], "First");
        assert_eq!(response.body["body"], "updated");
        assert_eq!(
            request!(app, alice, Method::POST, "/api/notes", json!({})).status,
            StatusCode::CREATED
        );
        let response = request!(
            app,
            alice,
            Method::GET,
            "/api/notes?limit=1&offset=1",
            json!({})
        );
        assert_eq!(response.body.as_array().unwrap().len(), 1);
        assert_eq!(
            request!(
                app,
                alice,
                Method::DELETE,
                &format!("/api/notes/{id}"),
                json!({})
            )
            .status,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            request!(
                app,
                alice,
                Method::GET,
                &format!("/api/notes/{id}"),
                json!({})
            )
            .status,
            StatusCode::NOT_FOUND
        );
    }

    #[actix_web::test]
    async fn invalid_content_and_pagination_return_bad_request() {
        let fixture = Fixture::new().await;
        let app = app!(fixture);
        let mut alice = Jar::new();
        signup!(app, alice, fixture, "alice@example.test");
        let response = request!(
            app,
            alice,
            Method::POST,
            "/api/notes",
            json!({"title":"First", "body":"hello"})
        );
        assert_eq!(response.status, StatusCode::CREATED);
        let id = response.body["id"].as_str().unwrap().to_owned();
        assert_eq!(
            request!(app, alice, Method::GET, "/api/notes?limit=0", json!({})).status,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            request!(
                app,
                alice,
                Method::POST,
                "/api/notes",
                json!({"title": "x".repeat(256)})
            )
            .status,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            request!(
                app,
                alice,
                Method::POST,
                "/api/notes",
                json!({"body": "x".repeat(102401)})
            )
            .status,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            request!(
                app,
                alice,
                Method::PATCH,
                &format!("/api/notes/{id}"),
                json!({})
            )
            .status,
            StatusCode::BAD_REQUEST
        );
        let stored = request!(
            app,
            alice,
            Method::GET,
            &format!("/api/notes/{id}"),
            json!({})
        );
        assert_eq!(stored.body["title"], "First");
        assert_eq!(stored.body["body"], "hello");
    }

    #[actix_web::test]
    async fn owners_and_anonymous_clients_cannot_access_private_notes() {
        let fixture = Fixture::new().await;
        let app = app!(fixture);
        let mut alice = Jar::new();
        signup!(app, alice, fixture, "alice@example.test");
        let response = request!(
            app,
            alice,
            Method::POST,
            "/api/notes",
            json!({"title":"First", "body":"hello"})
        );
        assert_eq!(response.status, StatusCode::CREATED);
        let id = response.body["id"].as_str().unwrap().to_owned();
        let mut bob = Jar::new();
        signup!(app, bob, fixture, "bob@example.test");
        for method in [Method::GET, Method::PATCH, Method::DELETE] {
            let response = request!(
                app,
                bob,
                method,
                &format!("/api/notes/{id}"),
                json!({"title":"stolen"})
            );
            assert_eq!(response.status, StatusCode::NOT_FOUND);
        }
        assert!(
            request!(app, bob, Method::GET, "/api/notes", json!({}))
                .body
                .as_array()
                .unwrap()
                .is_empty()
        );
        let mut anonymous = Jar::new();
        assert_eq!(
            request!(app, anonymous, Method::GET, "/api/notes", json!({})).status,
            StatusCode::UNAUTHORIZED
        );
    }
    #[actix_web::test]
    async fn create_sets_location_and_extractors_reject_malformed_requests() {
        let fixture = Fixture::new().await;
        let app = app!(fixture);
        let mut alice = Jar::new();
        signup!(app, alice, fixture, "alice@example.test");

        let req =
            crate::test_utils::request_builder(&alice, Method::POST, "/api/notes", &json!({}));
        let response = actix_web::test::call_service(&app, req.to_request()).await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let location = response
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let body: serde_json::Value = actix_web::test::read_body_json(response).await;
        assert_eq!(
            location,
            format!("/api/notes/{}", body["id"].as_str().unwrap())
        );
        assert_eq!(
            request!(app, alice, Method::GET, &location, json!({})).status,
            StatusCode::OK
        );
        for (method, path, body, expected) in [
            (
                Method::GET,
                "/api/notes/not-a-uuid",
                json!({}),
                StatusCode::NOT_FOUND,
            ),
            (
                Method::GET,
                "/api/notes?limit=invalid",
                json!({}),
                StatusCode::BAD_REQUEST,
            ),
            (
                Method::POST,
                "/api/notes",
                json!({"title":42}),
                StatusCode::BAD_REQUEST,
            ),
            (
                Method::POST,
                "/api/notes",
                json!({"user_id":"someone-else"}),
                StatusCode::BAD_REQUEST,
            ),
            (
                Method::PATCH,
                location.as_str(),
                json!({"unexpected":true}),
                StatusCode::BAD_REQUEST,
            ),
        ] {
            assert_eq!(
                request!(app, alice, method, path, body).status,
                expected,
                "{path}"
            );
        }
    }
    #[actix_web::test]
    async fn every_protected_endpoint_requires_authentication_with_valid_csrf() {
        let fixture = Fixture::new().await;
        let session = fixture.register("owner@example.test").await;
        let note = fixture
            .services
            .notes
            .create(
                session.user.id.clone(),
                crate::services::notes::NewNote {
                    title: "private".into(),
                    body: "original".into(),
                },
            )
            .await
            .unwrap();
        let app = app!(fixture);
        let mut jar = Jar::new();
        request!(app, jar, Method::GET, "/api/auth/csrf-token", json!({}));
        let path = format!("/api/notes/{}", note.id);
        for (method, path, body) in [
            (Method::GET, "/api/notes", json!({})),
            (Method::POST, "/api/notes", json!({"title":"unauthorized"})),
            (Method::GET, path.as_str(), json!({})),
            (Method::PATCH, path.as_str(), json!({"body":"unauthorized"})),
            (Method::DELETE, path.as_str(), json!({})),
            (Method::GET, "/api/user/me", json!({})),
        ] {
            let response = request!(app, jar, method, path, body);
            assert_eq!(response.status, StatusCode::UNAUTHORIZED, "{path}");
            assert_eq!(
                response.body,
                json!({"error":"Authentication required or credentials invalid"}),
                "{path}"
            );
            assert!(response.cookies.is_empty(), "{path}");
        }
        let notes = fixture
            .services
            .notes
            .list(
                session.user.id,
                crate::services::notes::Pagination {
                    limit: 50,
                    offset: 0,
                },
            )
            .await
            .unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].id, note.id);
        assert_eq!(notes[0].title, "private");
        assert_eq!(notes[0].body, "original");
    }
}
