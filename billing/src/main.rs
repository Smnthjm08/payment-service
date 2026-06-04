use actix_web::{
    post,
    web::{self, Data},
    middleware::Logger,
    App, HttpResponse, HttpServer, Responder,
};
use dotenv::dotenv;
use sqlx::{postgres::PgPoolOptions, Pool, Postgres};
use env_logger;

#[post("/echo")]
async fn echo(req_body: String) -> impl Responder {
    HttpResponse::Ok().body(req_body)
}

pub struct AppState {
    db: Pool<Postgres>,
}

async fn manual_hello() -> impl Responder {
    HttpResponse::Ok().body("Hey there!")
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
    // bind to PORT env or default to 8080; listen on all interfaces for containers
    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".into());
    let bind_addr = format!("0.0.0.0:{}", port);

    let server = HttpServer::new(move || {
        App::new()
            .app_data(Data::new(AppState { db: pool.clone() }))
            .wrap(Logger::default())
            .service(echo)
            .route("/hey", web::get().to(manual_hello))
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
