use actix_web::{App, HttpServer, middleware::Logger, web::Data};
use billing::{AppState, configure_routes, webhook_dispatcher};
use dotenv::dotenv;
use sqlx::postgres::PgPoolOptions;
use std::{sync::Arc, time::Duration};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();
    env_logger::init();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await
        .expect("Error connecting to pool");

    // Run any pending migrations automatically on startup.
    // This is idempotent — already-applied migrations are skipped.
    sqlx::migrate!("../migrations")
        .run(&pool)
        .await
        .expect("Failed to run database migrations");

    log::info!("Database migrations applied successfully");

    let psp_url = std::env::var("PSP_URL").unwrap_or_else(|_| "http://localhost:9090".into());
    log::info!("PSP URL: {}", psp_url);

    // Spawn the webhook dispatcher as a background task.
    // It polls for due deliveries every 5 seconds without blocking API responses.
    webhook_dispatcher::spawn(Arc::new(pool.clone()), Duration::from_secs(5));

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".into());
    let bind_addr = format!("0.0.0.0:{}", port);

    let server = HttpServer::new(move || {
        App::new()
            .app_data(Data::new(AppState {
                db: pool.clone(),
                psp_url: psp_url.clone(),
            }))
            .wrap(Logger::default())
            .configure(configure_routes)
    })
    .bind(&bind_addr)?
    .run();

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
