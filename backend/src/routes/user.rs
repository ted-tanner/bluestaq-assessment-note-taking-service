use actix_web::web;

use crate::handlers::{auth, users};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/user")
            .route("/me", web::get().to(auth::me))
            .route("/me", web::patch().to(users::update)),
    )
    .service(web::resource("/users/search").route(web::get().to(users::search)));
}
