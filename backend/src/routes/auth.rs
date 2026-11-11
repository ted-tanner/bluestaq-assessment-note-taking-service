use actix_web::web;

use crate::handlers::auth;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/auth")
            .route("/csrf-token", web::get().to(auth::csrf))
            .route("/register", web::post().to(auth::register))
            .route(
                "/register/verify",
                web::post().to(auth::verify_registration),
            )
            .route("/sign-in", web::post().to(auth::sign_in))
            .route("/2fa/email/verify", web::post().to(auth::verify_sign_in))
            .route("/email/resend", web::post().to(auth::resend))
            .route("/refresh", web::post().to(auth::refresh))
            .route("/logout", web::post().to(auth::logout))
            .route("/password-change", web::post().to(auth::change_password))
            .route(
                "/password-reset/request",
                web::post().to(auth::reset_request),
            )
            .route(
                "/password-reset/confirm",
                web::post().to(auth::reset_confirm),
            ),
    );
}
