use crate::{
    error::Error,
    middleware::auth::AuthenticatedUser,
    models::sharing::{Kind, ShareInput},
    services::AppServices,
};
use actix_csrf::extractor::{Csrf, CsrfHeader};
use actix_web::{HttpResponse, web};
use uuid::Uuid;

pub async fn list(
    user: AuthenticatedUser,
    services: web::Data<AppServices>,
    kind: web::Data<Kind>,
    id: web::Path<Uuid>,
) -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok().json(
        services
            .sharing
            .list(user.0.id, **kind, id.to_string())
            .await?,
    ))
}

pub async fn grant(
    _: Csrf<CsrfHeader>,
    user: AuthenticatedUser,
    services: web::Data<AppServices>,
    kind: web::Data<Kind>,
    path: web::Path<(Uuid, Uuid)>,
    input: web::Json<ShareInput>,
) -> Result<HttpResponse, Error> {
    let (id, recipient) = path.into_inner();
    services
        .sharing
        .grant(
            user.0.id,
            **kind,
            id.to_string(),
            recipient.to_string(),
            input.permission,
        )
        .await?;
    Ok(HttpResponse::NoContent().finish())
}

pub async fn revoke(
    _: Csrf<CsrfHeader>,
    user: AuthenticatedUser,
    services: web::Data<AppServices>,
    kind: web::Data<Kind>,
    path: web::Path<(Uuid, Uuid)>,
) -> Result<HttpResponse, Error> {
    let (id, recipient) = path.into_inner();
    services
        .sharing
        .revoke(user.0.id, **kind, id.to_string(), recipient.to_string())
        .await?;
    Ok(HttpResponse::NoContent().finish())
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    for (resource, kind) in [("notes", Kind::Note), ("folders", Kind::Folder)] {
        cfg.service(
            web::resource(format!("/{resource}/{{id}}/shares"))
                .app_data(web::Data::new(kind))
                .route(web::get().to(list)),
        )
        .service(
            web::resource(format!("/{resource}/{{id}}/shares/{{user_id}}"))
                .app_data(web::Data::new(kind))
                .route(web::put().to(grant))
                .route(web::delete().to(revoke)),
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::{Fixture, Jar, app, request, signup};
    use actix_web::http::{Method, StatusCode};
    use serde_json::json;

    #[actix_web::test]
    async fn inherited_and_direct_permissions_are_enforced_and_revoked() {
        let fixture = Fixture::new().await;
        let app = app!(fixture);
        let mut alice = Jar::new();
        let mut bob = Jar::new();
        let mut stranger = Jar::new();
        let a = signup!(app, alice, fixture, "alice@example.test");
        let b = signup!(app, bob, fixture, "bob@example.test");
        signup!(app, stranger, fixture, "stranger@example.test");
        let alice_id = a.body["user"]["id"].as_str().unwrap();
        let bob_id = b.body["user"]["id"].as_str().unwrap();
        let root = request!(
            app,
            alice,
            Method::POST,
            "/api/folders",
            json!({"name":"Private parent"})
        );
        let root_id = root.body["id"].as_str().unwrap();
        let shared = request!(
            app,
            alice,
            Method::POST,
            "/api/folders",
            json!({"name":"Shared subtree","parent_id":root_id})
        );
        let shared_id = shared.body["id"].as_str().unwrap();
        let folder_url = format!("/api/folders/{shared_id}");
        let share_url = format!("{folder_url}/shares/{bob_id}");
        let note = request!(
            app,
            alice,
            Method::POST,
            "/api/notes",
            json!({"title":"Astronomy","body":"stars","folder_id":shared_id})
        );
        let id = note.body["id"].as_str().unwrap();
        let note_url = format!("/api/notes/{id}");
        let note_share = format!("{note_url}/shares/{bob_id}");
        request!(
            app,
            alice,
            Method::POST,
            "/api/notes",
            json!({"title":"Private astronomy"})
        );
        assert_eq!(
            request!(app, bob, Method::GET, &note_url, json!({})).status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            request!(
                app,
                alice,
                Method::PUT,
                &share_url,
                json!({"permission":"view"})
            )
            .status,
            StatusCode::NO_CONTENT
        );
        let folders = request!(app, bob, Method::GET, "/api/folders", json!({}));
        assert_eq!(folders.body.as_array().unwrap().len(), 1);
        assert!(folders.body[0]["parent_id"].is_null());
        assert_eq!(folders.body[0]["access"], "view");
        assert_eq!(folders.body[0]["owner"]["id"], alice_id);
        let fetched = request!(app, bob, Method::GET, &note_url, json!({}));
        assert_eq!(fetched.body["access"], "view");
        assert_eq!(fetched.body["owner"]["email"], "alice@example.test");
        let result = request!(
            app,
            bob,
            Method::GET,
            "/api/notes/search?q=astronomy&folder_id=shared&limit=1",
            json!({})
        );
        assert_eq!(result.body["items"].as_array().unwrap().len(), 1);
        assert_eq!(result.body["has_more"], false);
        assert_eq!(
            request!(
                app,
                bob,
                Method::GET,
                &format!("/api/notes?folder_id={root_id}"),
                json!({})
            )
            .status,
            StatusCode::NOT_FOUND
        );
        for (method, path, body, revision) in [
            (
                Method::PATCH,
                note_url.clone(),
                json!({"body":"stolen"}),
                "\"1\"",
            ),
            (Method::DELETE, note_url.clone(), json!({}), "\"1\""),
            (
                Method::POST,
                "/api/notes".into(),
                json!({"folder_id":shared_id}),
                "\"1\"",
            ),
            (
                Method::POST,
                "/api/folders".into(),
                json!({"name":"Denied","parent_id":shared_id}),
                "\"1\"",
            ),
            (
                Method::PATCH,
                folder_url.clone(),
                json!({"name":"Denied"}),
                "\"1\"",
            ),
            (Method::DELETE, folder_url.clone(), json!({}), "\"1\""),
            (
                Method::GET,
                format!("{folder_url}/shares"),
                json!({}),
                "\"1\"",
            ),
            (
                Method::PUT,
                note_share.clone(),
                json!({"permission":"edit"}),
                "\"1\"",
            ),
        ] {
            assert_eq!(
                request!(app, bob, method, &path, body, revision).status,
                StatusCode::FORBIDDEN,
                "{path}"
            );
        }
        // A direct note grant overrides an inherited view grant, but exposes no parent.
        assert_eq!(
            request!(
                app,
                alice,
                Method::PUT,
                &note_share,
                json!({"permission":"edit"})
            )
            .status,
            StatusCode::NO_CONTENT
        );
        let edited = request!(
            app,
            bob,
            Method::PATCH,
            &note_url,
            json!({"body":"updated stars","folder_id":shared_id}),
            "\"1\""
        );
        assert_eq!(edited.body["access"], "edit");
        assert_eq!(edited.body["revision"], 2);
        assert_eq!(
            request!(
                app,
                bob,
                Method::PATCH,
                &note_url,
                json!({"folder_id":null}),
                "\"2\""
            )
            .status,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            request!(
                app,
                alice,
                Method::PUT,
                &share_url,
                json!({"permission":"edit"})
            )
            .status,
            StatusCode::NO_CONTENT
        );
        let created = request!(
            app,
            bob,
            Method::POST,
            "/api/notes",
            json!({"title":"Future content","folder_id":shared_id})
        );
        assert_eq!(created.status, StatusCode::CREATED);
        assert_eq!(created.body["owner"]["id"], alice_id);
        assert_eq!(created.body["access"], "edit");
        let created_id = created.body["id"].as_str().unwrap();
        let child = request!(
            app,
            bob,
            Method::POST,
            "/api/folders",
            json!({"name":"Future subfolder","parent_id":shared_id})
        );
        assert_eq!(child.status, StatusCode::CREATED);
        let child_id = child.body["id"].as_str().unwrap();
        assert_eq!(
            request!(
                app,
                bob,
                Method::PATCH,
                &folder_url,
                json!({"name":"Renamed"}),
                "\"1\""
            )
            .status,
            StatusCode::OK
        );
        assert_eq!(
            request!(
                app,
                bob,
                Method::PATCH,
                &folder_url,
                json!({"parent_id":null}),
                "\"2\""
            )
            .status,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            request!(
                app,
                bob,
                Method::DELETE,
                &format!("/api/notes/{created_id}"),
                json!({}),
                "\"1\""
            )
            .status,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            request!(app, stranger, Method::GET, &note_url, json!({})).status,
            StatusCode::NOT_FOUND
        );
        // Removing one grant retains independent grants. Removing all grants blocks saves.
        assert_eq!(
            request!(app, alice, Method::DELETE, &share_url, json!({})).status,
            StatusCode::NO_CONTENT
        );
        assert!(
            request!(app, bob, Method::GET, "/api/folders", json!({}))
                .body
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            request!(app, bob, Method::GET, &note_url, json!({})).body["access"],
            "edit"
        );
        assert_eq!(
            request!(
                app,
                bob,
                Method::GET,
                &format!("/api/notes/{}", created_id),
                json!({})
            )
            .status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            request!(app, alice, Method::DELETE, &note_share, json!({})).status,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            request!(
                app,
                bob,
                Method::PATCH,
                &note_url,
                json!({"body":"revoked"}),
                "\"2\""
            )
            .status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            request!(app, alice, Method::GET, &note_url, json!({})).body["body"],
            "updated stars"
        );
        // A share on a parent follows moves dynamically and covers future descendants.
        request!(
            app,
            alice,
            Method::PUT,
            &share_url,
            json!({"permission":"view"})
        );
        let descendant = request!(
            app,
            alice,
            Method::POST,
            "/api/notes",
            json!({"title":"New descendant","folder_id":child_id})
        );
        let descendant_url = format!("/api/notes/{}", descendant.body["id"].as_str().unwrap());
        assert_eq!(
            request!(app, bob, Method::GET, &descendant_url, json!({})).status,
            StatusCode::OK
        );
        request!(
            app,
            alice,
            Method::PATCH,
            &format!("/api/folders/{child_id}"),
            json!({"parent_id":null}),
            "\"1\""
        );
        assert_eq!(
            request!(app, bob, Method::GET, &descendant_url, json!({})).status,
            StatusCode::NOT_FOUND
        );
        let grants = request!(
            app,
            alice,
            Method::GET,
            &format!("{folder_url}/shares"),
            json!({})
        );
        assert_eq!(grants.body[0]["user"]["id"], bob_id);
        assert_eq!(
            request!(app, alice, Method::DELETE, &folder_url, json!({}), "\"2\"").status,
            StatusCode::NO_CONTENT
        );
        assert!(
            request!(app, bob, Method::GET, "/api/folders", json!({}))
                .body
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[actix_web::test]
    async fn sharing_rejects_invalid_recipients_permissions_and_unauthenticated_requests() {
        let fixture = Fixture::new().await;
        let app = app!(fixture);
        let mut owner = Jar::new();
        let a = signup!(app, owner, fixture, "owner@example.test");
        let id = a.body["user"]["id"].as_str().unwrap();
        let note = request!(
            app,
            owner,
            Method::POST,
            "/api/notes",
            json!({"title":"Private"})
        );
        let base = format!("/api/notes/{}/shares", note.body["id"].as_str().unwrap());
        assert_eq!(
            request!(
                app,
                owner,
                Method::PUT,
                &format!("{base}/{id}"),
                json!({"permission":"view"})
            )
            .status,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            request!(
                app,
                owner,
                Method::PUT,
                &format!("{base}/{}", uuid::Uuid::new_v4()),
                json!({"permission":"view"})
            )
            .status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            request!(
                app,
                owner,
                Method::PUT,
                &format!("{base}/{id}"),
                json!({"permission":"owner"})
            )
            .status,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            request!(
                app,
                owner,
                Method::PUT,
                &format!("{base}/{id}"),
                json!({"permission":"view","user_id":id})
            )
            .status,
            StatusCode::BAD_REQUEST
        );
        let mut anonymous = Jar::new();
        assert_eq!(
            request!(app, anonymous, Method::GET, &base, json!({})).status,
            StatusCode::UNAUTHORIZED
        );
        request!(
            app,
            anonymous,
            Method::GET,
            "/api/auth/csrf-token",
            json!({})
        );
        assert_eq!(
            request!(
                app,
                anonymous,
                Method::PUT,
                &format!("{base}/{id}"),
                json!({"permission":"view"})
            )
            .status,
            StatusCode::UNAUTHORIZED
        );
    }
}
