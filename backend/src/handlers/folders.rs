use crate::{
    error::Error,
    middleware::auth::AuthenticatedUser,
    models::folder::Folder,
    services::{
        AppServices,
        folders::{FolderPatch, NewFolder},
    },
};
use actix_csrf::extractor::{Csrf, CsrfHeader};
use actix_web::{HttpRequest, HttpResponse, web};
use uuid::Uuid;

fn response(folder: Folder, created: bool) -> HttpResponse {
    let mut response = if created {
        HttpResponse::Created()
    } else {
        HttpResponse::Ok()
    };
    response.append_header(("ETag", format!("\"{}\"", folder.revision)));
    if created {
        response.append_header(("Location", format!("/api/folders/{}", folder.id)));
    }
    response.json(folder)
}

pub async fn list(
    user: AuthenticatedUser,
    services: web::Data<AppServices>,
) -> Result<HttpResponse, Error> {
    let folders = services.folders.list(user.0.id.clone()).await?;
    Ok(HttpResponse::Ok().json(services.sharing.present_folders(user.0.id, folders).await?))
}

pub async fn create(
    _: Csrf<CsrfHeader>,
    user: AuthenticatedUser,
    services: web::Data<AppServices>,
    input: web::Json<NewFolder>,
) -> Result<HttpResponse, Error> {
    Ok(response(
        services
            .folders
            .create(user.0.id, input.into_inner())
            .await?,
        true,
    ))
}

pub async fn update(
    req: HttpRequest,
    _: Csrf<CsrfHeader>,
    user: AuthenticatedUser,
    services: web::Data<AppServices>,
    id: web::Path<Uuid>,
    input: web::Json<FolderPatch>,
) -> Result<HttpResponse, Error> {
    Ok(response(
        services
            .folders
            .update(
                user.0.id,
                id.to_string(),
                super::notes::revision(&req)?,
                input.into_inner(),
            )
            .await?,
        false,
    ))
}

pub async fn delete(
    req: HttpRequest,
    _: Csrf<CsrfHeader>,
    user: AuthenticatedUser,
    services: web::Data<AppServices>,
    id: web::Path<Uuid>,
) -> Result<HttpResponse, Error> {
    services
        .folders
        .delete(user.0.id, id.to_string(), super::notes::revision(&req)?)
        .await?;
    Ok(HttpResponse::NoContent().finish())
}

#[cfg(test)]
mod tests {
    use crate::test_utils::{Fixture, Jar, app, request, signup};
    use actix_web::http::{Method, StatusCode};
    use serde_json::json;

    #[actix_web::test]
    async fn tree_moves_scoped_search_and_recursive_deletion() {
        let fixture = Fixture::new().await;
        let app = app!(fixture);
        let mut alice = Jar::new();
        signup!(app, alice, fixture, "alice@example.test");
        let parent = request!(
            app,
            alice,
            Method::POST,
            "/api/folders",
            json!({"name":" Projects "})
        );
        assert_eq!(parent.status, StatusCode::CREATED);
        assert_eq!(parent.body["name"], "Projects");
        assert!(parent.body.get("user_id").is_none());
        let parent_id = parent.body["id"].as_str().unwrap();
        let parent_url = format!("/api/folders/{parent_id}");
        let child = request!(
            app,
            alice,
            Method::POST,
            "/api/folders",
            json!({"name":"Nested", "parent_id":parent_id})
        );
        let child_id = child.body["id"].as_str().unwrap();
        let child_url = format!("/api/folders/{child_id}");
        let note = request!(
            app,
            alice,
            Method::POST,
            "/api/notes",
            json!({"title":"searchable child", "folder_id":child_id})
        );
        assert_eq!(note.status, StatusCode::CREATED);
        let note_url = format!("/api/notes/{}", note.body["id"].as_str().unwrap());
        request!(
            app,
            alice,
            Method::POST,
            "/api/notes",
            json!({"title":"searchable unfiled"})
        );
        request!(
            app,
            alice,
            Method::POST,
            "/api/notes",
            json!({"title":"searchable parent", "folder_id":parent_id})
        );
        let direct = request!(
            app,
            alice,
            Method::GET,
            &format!("/api/notes?folder_id={parent_id}"),
            json!({})
        );
        assert_eq!(direct.body.as_array().unwrap().len(), 1);
        let recursive = request!(
            app,
            alice,
            Method::GET,
            &format!(
                "/api/notes/search?q=searchable&folder_id={parent_id}&include_descendants=true&limit=1"
            ),
            json!({})
        );
        assert_eq!(recursive.body["items"].as_array().unwrap().len(), 1);
        assert_eq!(recursive.body["has_more"], true);
        let second = request!(
            app,
            alice,
            Method::GET,
            &format!(
                "/api/notes/search?q=searchable&folder_id={parent_id}&include_descendants=true&limit=1&offset=1"
            ),
            json!({})
        );
        assert_ne!(
            recursive.body["items"][0]["id"],
            second.body["items"][0]["id"]
        );
        assert_eq!(second.body["has_more"], false);
        let root = request!(
            app,
            alice,
            Method::GET,
            "/api/notes/search?folder_id=root",
            json!({})
        );
        assert_eq!(root.body["items"].as_array().unwrap().len(), 1);
        // Omitted relationship preserves it; explicit null clears it.
        let updated = request!(
            app,
            alice,
            Method::PATCH,
            &note_url,
            json!({"title":"searchable edited"}),
            "\"1\""
        );
        assert_eq!(updated.body["folder_id"], child_id);
        let moved = request!(
            app,
            alice,
            Method::PATCH,
            &note_url,
            json!({"folder_id":null}),
            "\"2\""
        );
        assert_eq!(moved.body["folder_id"], serde_json::Value::Null);
        assert_eq!(moved.body["revision"], 3);
        assert_eq!(
            request!(
                app,
                alice,
                Method::PATCH,
                &note_url,
                json!({"folder_id":child_id}),
                "\"2\""
            )
            .status,
            StatusCode::PRECONDITION_FAILED
        );
        request!(
            app,
            alice,
            Method::PATCH,
            &note_url,
            json!({"folder_id":child_id}),
            "\"3\""
        );
        // Renaming leaves the parent intact; moving a parent under its child is rejected.
        let renamed = request!(
            app,
            alice,
            Method::PATCH,
            &child_url,
            json!({"name":"Renamed"}),
            "\"1\""
        );
        assert_eq!(renamed.body["parent_id"], parent_id);
        for destination in [parent_id, child_id] {
            assert_eq!(
                request!(
                    app,
                    alice,
                    Method::PATCH,
                    &parent_url,
                    json!({"parent_id":destination}),
                    "\"1\""
                )
                .status,
                StatusCode::BAD_REQUEST
            );
        }
        let unparented = request!(
            app,
            alice,
            Method::PATCH,
            &child_url,
            json!({"parent_id":null}),
            "\"2\""
        );
        assert!(unparented.body["parent_id"].is_null());
        request!(
            app,
            alice,
            Method::PATCH,
            &child_url,
            json!({"parent_id":parent_id}),
            "\"3\""
        );
        assert_eq!(
            request!(app, alice, Method::DELETE, &parent_url, json!({}), "\"2\"").status,
            StatusCode::PRECONDITION_FAILED
        );
        assert_eq!(
            request!(app, alice, Method::DELETE, &parent_url, json!({}), "\"1\"").status,
            StatusCode::NO_CONTENT
        );
        assert!(
            request!(app, alice, Method::GET, "/api/folders", json!({}))
                .body
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            request!(app, alice, Method::GET, &note_url, json!({})).status,
            StatusCode::NOT_FOUND
        );
        let remaining = request!(
            app,
            alice,
            Method::GET,
            "/api/notes/search?q=searchable",
            json!({})
        );
        assert_eq!(remaining.body["items"].as_array().unwrap().len(), 1);
        assert_eq!(remaining.body["items"][0]["title"], "searchable unfiled");
    }

    #[actix_web::test]
    async fn folders_are_private_and_validate_destinations_and_payloads() {
        let fixture = Fixture::new().await;
        let app = app!(fixture);
        let mut alice = Jar::new();
        signup!(app, alice, fixture, "alice@example.test");
        let folder = request!(
            app,
            alice,
            Method::POST,
            "/api/folders",
            json!({"name":"Private"})
        );
        let id = folder.body["id"].as_str().unwrap();
        let url = format!("/api/folders/{id}");
        let mut bob = Jar::new();
        signup!(app, bob, fixture, "bob@example.test");
        assert!(
            request!(app, bob, Method::GET, "/api/folders", json!({}))
                .body
                .as_array()
                .unwrap()
                .is_empty()
        );
        for method in [Method::PATCH, Method::DELETE] {
            assert_eq!(
                request!(app, bob, method, &url, json!({"name":"Stolen"}), "\"1\"").status,
                StatusCode::NOT_FOUND
            );
        }
        for (method, path, body) in [
            (
                Method::POST,
                "/api/folders".to_owned(),
                json!({"name":"Child", "parent_id":id}),
            ),
            (
                Method::POST,
                "/api/notes".to_owned(),
                json!({"folder_id":id}),
            ),
            (Method::GET, format!("/api/notes?folder_id={id}"), json!({})),
            (
                Method::GET,
                format!("/api/notes/search?q=secret&folder_id={id}&include_descendants=true"),
                json!({}),
            ),
        ] {
            assert_eq!(
                request!(app, bob, method, &path, body).status,
                StatusCode::NOT_FOUND
            );
        }
        let own = request!(
            app,
            bob,
            Method::POST,
            "/api/folders",
            json!({"name":"Own"})
        );
        let own_url = format!("/api/folders/{}", own.body["id"].as_str().unwrap());
        assert_eq!(
            request!(
                app,
                bob,
                Method::PATCH,
                &own_url,
                json!({"parent_id":id}),
                "\"1\""
            )
            .status,
            StatusCode::NOT_FOUND
        );
        let note = request!(app, bob, Method::POST, "/api/notes", json!({}));
        let note_url = format!("/api/notes/{}", note.body["id"].as_str().unwrap());
        assert_eq!(
            request!(
                app,
                bob,
                Method::PATCH,
                &note_url,
                json!({"folder_id":id}),
                "\"1\""
            )
            .status,
            StatusCode::NOT_FOUND
        );
        for body in [
            json!({"name":"  "}),
            json!({"name":"x".repeat(256)}),
            json!({"name":42}),
            json!({"name":"Name", "user_id":"alice"}),
        ] {
            assert_eq!(
                request!(app, alice, Method::POST, "/api/folders", body).status,
                StatusCode::BAD_REQUEST
            );
        }
        assert_eq!(
            request!(
                app,
                alice,
                Method::POST,
                "/api/folders",
                json!({"name":"é".repeat(255)})
            )
            .status,
            StatusCode::CREATED
        );
        assert_eq!(
            request!(
                app,
                alice,
                Method::POST,
                "/api/folders",
                json!({"name":"Private"})
            )
            .status,
            StatusCode::CREATED
        );
        assert_eq!(
            request!(app, alice, Method::PATCH, &url, json!({"name":"Rename"})).status,
            StatusCode::PRECONDITION_REQUIRED
        );
        assert_eq!(
            request!(
                app,
                alice,
                Method::PATCH,
                &url,
                json!({"name":"Rename"}),
                "invalid"
            )
            .status,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            request!(app, alice, Method::PATCH, &url, json!({}), "\"1\"").status,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            request!(app, alice, Method::DELETE, &url, json!({})).status,
            StatusCode::PRECONDITION_REQUIRED
        );
        let mut anonymous = Jar::new();
        assert_eq!(
            request!(app, anonymous, Method::GET, "/api/folders", json!({})).status,
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
                Method::POST,
                "/api/folders",
                json!({"name":"Anonymous"})
            )
            .status,
            StatusCode::UNAUTHORIZED
        );
        let no_csrf = actix_web::test::TestRequest::post()
            .uri("/api/folders")
            .set_json(json!({"name":"Invalid"}))
            .to_request();
        assert_eq!(
            actix_web::test::call_service(&app, no_csrf).await.status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
}
