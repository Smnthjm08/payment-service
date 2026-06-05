use actix_web::{App, HttpResponse, HttpServer, Responder, middleware::Logger, web};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct ChargeRequest {
    card_token: String,
    #[allow(dead_code)]
    amount_cents: i64,
    #[allow(dead_code)]
    idempotency_key: Option<String>,
}

/// Success:  { "status": "succeeded", "psp_ref": "<uuid>" }
/// Failure:  { "status": "failed",    "code": "<reason>"  }
#[derive(Debug, Serialize)]
struct ChargeResponse {
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    psp_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'static str>,
}

impl ChargeResponse {
    fn succeeded() -> Self {
        Self {
            status: "succeeded",
            psp_ref: Some(format!("psp_{}", Uuid::new_v4())),
            code: None,
        }
    }

    fn failed(code: &'static str) -> Self {
        Self {
            status: "failed",
            psp_ref: None,
            code: Some(code),
        }
    }
}

async fn charge(payload: web::Json<ChargeRequest>) -> impl Responder {
    let token = payload.card_token.trim();

    match token {
        // Returns succeeded after ~100 ms.
        "tok_success" => {
            sleep(Duration::from_millis(100)).await;
            HttpResponse::Ok().json(ChargeResponse::succeeded())
        }

        // insufficient_funds after ~100 ms.
        "tok_insufficient_funds" => {
            sleep(Duration::from_millis(100)).await;
            HttpResponse::Ok().json(ChargeResponse::failed("insufficient_funds"))
        }

        // ── tok_card_declined ─────────────────────────────────────────────────
        "tok_card_declined" => {
            sleep(Duration::from_millis(100)).await;
            HttpResponse::Ok().json(ChargeResponse::failed("card_declined"))
        }

        // Returns success after 30 seconds.
        "tok_timeout" => {
            sleep(Duration::from_secs(30)).await;
            HttpResponse::Ok().json(ChargeResponse::succeeded())
        }

        // Returns HTTP 500 — billing service must recover gracefully.
        "tok_network_error" => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": "gateway_error",
            "message": "An internal error occurred in the payment gateway"
        })),

        _ => {
            sleep(Duration::from_millis(100)).await;
            HttpResponse::Ok().json(ChargeResponse::succeeded())
        }
    }
}

async fn health() -> impl Responder {
    HttpResponse::Ok().body("mock-psp healthy")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    let port = std::env::var("MOCK_PSP_PORT").unwrap_or_else(|_| "9090".into());
    let bind_addr = format!("0.0.0.0:{}", port);

    log::info!("mock-psp listening on {}", bind_addr);

    HttpServer::new(|| {
        App::new()
            .wrap(Logger::default())
            .route("/health", web::get().to(health))
            .route("/charge", web::post().to(charge))
    })
    .bind(&bind_addr)?
    .run()
    .await
}
