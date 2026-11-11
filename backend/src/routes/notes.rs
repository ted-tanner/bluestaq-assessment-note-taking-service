use actix_web::web;

use crate::handlers::notes;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("/notes")
            .route(web::post().to(notes::create))
            .route(web::get().to(notes::list)),
    )
    .service(
        web::resource("/notes/{id}")
            .route(web::get().to(notes::get))
            .route(web::patch().to(notes::update))
            .route(web::delete().to(notes::delete)),
    );
}
