use crate::{error::Error, middleware::auth::AuthenticatedUser, services::AppServices};
use actix_csrf::extractor::{Csrf, CsrfHeader};
use actix_web::{HttpResponse, web};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Search {
    pub q: String,
}

pub async fn update(
    _: Csrf<CsrfHeader>,
    user: AuthenticatedUser,
    services: web::Data<AppServices>,
    input: web::Json<Profile>,
) -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok().json(
        services
            .users
            .rename(user.0.id, input.into_inner().name)
            .await?,
    ))
}

pub async fn search(
    user: AuthenticatedUser,
    services: web::Data<AppServices>,
    input: web::Query<Search>,
) -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok().json(
        services
            .users
            .search(user.0.id, input.into_inner().q)
            .await?,
    ))
}

#[cfg(test)]
mod tests {
    use crate::test_utils::{Fixture, Jar, app, request, signup};
    use actix_web::http::{Method, StatusCode};
    use serde_json::json;
    #[actix_web::test]
    async fn names_profiles_and_literal_search_return_only_safe_verified_users() {
        let fixture = Fixture::new().await;
        let app = app!(fixture);
        let mut alice = Jar::new();
        let mut bob = Jar::new();
        signup!(app, alice, fixture, "alice@example.test");
        signup!(app, bob, fixture, "bob@example.test");
        let result = request!(
            app,
            bob,
            Method::PATCH,
            "/api/user/me",
            json!({"name":" Élodie Example "})
        );
        assert_eq!(result.body["name"], "Élodie Example");
        assert!(result.body.get("password_hash").is_none());
        assert_eq!(
            request!(app, bob, Method::GET, "/api/user/me", json!({})).body["name"],
            "Élodie Example"
        );
        for q in ["élodie", "ÉLODIE", "BOB@EXAMPLE"] {
            let q = q.replace('É', "%C3%89").replace('é', "%C3%A9");
            let results = request!(
                app,
                alice,
                Method::GET,
                &format!("/api/users/search?q={q}"),
                json!({})
            );
            assert_eq!(results.body.as_array().unwrap().len(), 1, "{q}");
            assert_eq!(results.body[0]["email"], "bob@example.test");
            assert_eq!(results.body[0].as_object().unwrap().len(), 3);
        }
        assert!(
            request!(app, bob, Method::GET, "/api/users/search?q=bob", json!({}))
                .body
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            request!(
                app,
                alice,
                Method::GET,
                "/api/users/search?q=%25%25",
                json!({})
            )
            .body
            .as_array()
            .unwrap()
            .is_empty()
        );
        request!(
            app,
            alice,
            Method::POST,
            "/api/auth/register",
            json!({"email":"pending@example.test","password":crate::test_utils::PASSWORD,"name":"Pending Person"})
        );
        assert!(
            request!(
                app,
                alice,
                Method::GET,
                "/api/users/search?q=pending",
                json!({})
            )
            .body
            .as_array()
            .unwrap()
            .is_empty()
        );
        for name in [" ".to_owned(), "x".repeat(101)] {
            assert_eq!(
                request!(
                    app,
                    bob,
                    Method::PATCH,
                    "/api/user/me",
                    json!({"name":name})
                )
                .status,
                StatusCode::BAD_REQUEST
            );
        }
        assert_eq!(
            request!(app, bob, Method::GET, "/api/users/search?q=a", json!({})).status,
            StatusCode::BAD_REQUEST
        );
        let mut anonymous = Jar::new();
        assert_eq!(
            request!(
                app,
                anonymous,
                Method::GET,
                "/api/users/search?q=bob",
                json!({})
            )
            .status,
            StatusCode::UNAUTHORIZED
        );
        let missing_name = crate::test_utils::request_builder(
            &alice,
            Method::POST,
            "/api/auth/register",
            &json!({}),
        )
        .set_json(json!({"email":"missing@example.test","password":crate::test_utils::PASSWORD}));
        assert_eq!(
            actix_web::test::call_service(&app, missing_name.to_request())
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(request!(app,alice,Method::POST,"/api/auth/register",json!({"email":"blank@example.test","password":crate::test_utils::PASSWORD,"name":" "})).status,StatusCode::BAD_REQUEST);
    }
}
