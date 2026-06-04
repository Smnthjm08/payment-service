pub mod domains;
pub mod handlers;
pub mod middlewares;
pub mod psp_client;
pub mod repositories;
pub mod webhook_dispatcher;

use actix_web::{HttpResponse, Responder, web};
use sqlx::{Pool, Postgres};

/// Shared application state injected via `actix_web::web::Data`.
pub struct AppState {
    /// Database connection pool.
    pub db: Pool<Postgres>,
    /// Base URL of the PSP (e.g. `http://localhost:9090`).
    pub psp_url: String,
}

async fn health_handler() -> impl Responder {
    HttpResponse::Ok().body("Healthy!")
}

/// Register all API routes onto the given `ServiceConfig`.
///
/// Used by both the production server (`main.rs`) and integration tests,
/// so the routing is defined exactly once.
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api")
            .route("/health", web::get().to(health_handler))
            .service(handlers::businesses::businesses_scope())
            .service(handlers::customers::customers_scope())
            .service(handlers::invoices::invoices_scope())
            .service(handlers::webhooks::webhooks_scope()),
    );
}
