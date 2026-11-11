use actix_web::HttpResponse;

pub async fn health() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({"status": "ok"}))
}

#[cfg(test)]
mod tests {
    use actix_web::{App, http::StatusCode, test};

    #[actix_web::test]
    async fn health_returns_json_without_authentication_or_database_services() {
        let config = crate::test_utils::config();
        let app =
            test::init_service(App::new().configure(|cfg| crate::routes::configure(cfg, &config)))
                .await;

        let response = test::call_service(
            &app,
            test::TestRequest::get().uri("/api/health").to_request(),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "application/json"
        );
        assert_eq!(response.response().cookies().count(), 0);
        let body: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(body, serde_json::json!({"status": "ok"}));
    }
}
