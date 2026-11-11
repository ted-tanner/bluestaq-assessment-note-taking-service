use actix_web::{
    App, HttpServer,
    middleware::{DefaultHeaders, Logger},
    web,
};
use bluestaq_notes::{
    config::Config,
    db, routes,
    services::{AppServices, email::senders::MockSender},
};
use std::sync::Arc;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let config = Arc::new(Config::from_env()?);
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let database_url = config.database_url.clone();
    let pool = web::block(move || db::connect(&database_url))
        .await
        .map_err(std::io::Error::other)??;

    let services = AppServices::new(pool, config.clone(), Arc::new(MockSender::default()))
        .await
        .map_err(std::io::Error::other)?;
    let services = web::Data::new(services);

    log::info!("Starting Bluestaq Notes API on {}", config.bind);
    let app_config = config.clone();

    HttpServer::new(move || {
        App::new()
            .app_data(services.clone())
            .wrap(Logger::new("%a %m %U %s %Dms"))
            .wrap(
                DefaultHeaders::new()
                    .add(("Cache-Control", "no-store"))
                    .add(("X-Content-Type-Options", "nosniff")),
            )
            .configure(|cfg| routes::configure(cfg, &app_config))
    })
    .bind(&config.bind)?
    .run()
    .await
}
