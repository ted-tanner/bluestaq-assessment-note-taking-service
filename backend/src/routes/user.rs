use actix_web::web;

use crate::handlers::auth;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(web::scope("/user").route("/me", web::get().to(auth::me)));
}
