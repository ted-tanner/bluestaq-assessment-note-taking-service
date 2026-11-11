use crate::handlers::folders;
use actix_web::web;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("/folders")
            .route(web::get().to(folders::list))
            .route(web::post().to(folders::create)),
    )
    .service(
        web::resource("/folders/{id}")
            .route(web::patch().to(folders::update))
            .route(web::delete().to(folders::delete)),
    );
}
