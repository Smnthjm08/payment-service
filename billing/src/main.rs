use actix_web::{
    App, HttpResponse, HttpServer, Responder,
    middleware::Logger,
    web::{self, Data},
};
use dotenv::dotenv;
use env_logger;
use sqlx::{Pool, Postgres, postgres::PgPoolOptions};
use std::sync::Arc;
use std::time::Duration;

pub mod domains;
pub mod handlers;
pub mod middlewares;
pub mod psp_client;
pub mod repositories;
pub mod webhook_dispatcher;

pub struct AppState {
    pub(crate) db: Pool<Postgres>,
    pub(crate) psp_url: String,
}

async fn manual_hello() -> impl Responder {
    HttpResponse::Ok().body("Healthy!")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();

    // init logging
    env_logger::init();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await
        .expect("Error connecting to pool");

    let psp_url = std::env::var("PSP_URL").unwrap_or_else(|_| "http://localhost:9090".into());
    log::info!("PSP URL: {}", psp_url);

    webhook_dispatcher::spawn(Arc::new(pool.clone()), Duration::from_secs(5));

    // bind to PORT env or default to 8080; listen on all interfaces for containers
    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".into());
    let bind_addr = format!("0.0.0.0:{}", port);

    let server = HttpServer::new(move || {
        App::new()
            .app_data(Data::new(AppState { db: pool.clone(), psp_url: psp_url.clone() }))
            .wrap(Logger::default())
            .service(
                web::scope("/api")
                    .route("/health", web::get().to(manual_hello))
                    .service(handlers::businesses::businesses_scope())
                    .service(handlers::customers::customers_scope())
                    .service(handlers::invoices::invoices_scope())
                    .service(handlers::webhooks::webhooks_scope()),
            )
    })
    .bind(&bind_addr)?
    .run();

    // handle to control the server (stop/pause)
    let handle = server.handle();
    actix_web::rt::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to listen for ctrl_c signal");
        log::info!("Shutdown signal received, stopping server");
        handle.stop(true).await;
    });

    server.await
}
