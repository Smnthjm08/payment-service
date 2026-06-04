//! Payment-critical integration tests.
//!
//! Each test spins up three in-process services on ephemeral ports:
//!   1. An isolated PostgreSQL schema  (via `#[sqlx::test]`)
//!   2. A tiny mock PSP               (inline actix-web server)
//!   3. The billing HTTP server        (same app factory as production)
//!
//! **Prerequisites**
//! `DATABASE_URL` must point to a running PostgreSQL server before running:
//! ```
//! export DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres
//! cargo test --test payment_integration_tests
//! ```
//!
//! Three required tests
//! --------------------
//! 1. `test_concurrent_pay_only_one_succeeds`        — concurrency / double-charge
//! 2. `test_idempotent_pay_replays_without_second_psp_call` — idempotency
//! 3. `test_psp_timeout_invoice_not_stuck_in_processing`    — PSP failure (timeout)
//!
//! A fourth test covers `tok_network_error` (HTTP-500) for completeness.

use actix_web::{App, HttpServer, web, web::Data};
use billing::{AppState, configure_routes};
use futures::future::join_all;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::{
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use uuid::Uuid;

// ── Seed helpers ─────────────────────────────────────────────────────────────

fn sha256_hex(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

/// Insert a business + active API key. Returns `(business_id, raw_api_key)`.
async fn seed_business(pool: &PgPool) -> (Uuid, String) {
    let business_id = Uuid::new_v4();
    let prefix = format!("dodo_live_{}", Uuid::new_v4().simple());
    let secret = Uuid::new_v4().simple().to_string();
    let raw_key = format!("{}.{}", prefix, secret);
    let key_hash = sha256_hex(&raw_key);

    sqlx::query!(
        "INSERT INTO businesses (id, email, name) VALUES ($1, $2, $3)",
        business_id,
        format!("test-{}@example.com", business_id),
        "Test Business",
    )
    .execute(pool)
    .await
    .unwrap();

    sqlx::query!(
        "INSERT INTO api_keys (id, business_id, key_prefix, key_hash) VALUES ($1, $2, $3, $4)",
        Uuid::new_v4(),
        business_id,
        prefix,
        key_hash,
    )
    .execute(pool)
    .await
    .unwrap();

    (business_id, raw_key)
}

/// Insert a customer scoped to `business_id`.
async fn seed_customer(pool: &PgPool, business_id: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query!(
        "INSERT INTO customers (id, business_id, name, email) VALUES ($1, $2, $3, $4)",
        id,
        business_id,
        "Test Customer",
        format!("cust-{}@example.com", id),
    )
    .execute(pool)
    .await
    .unwrap();
    id
}

/// Insert an invoice in `Open` state with a single £10 line item.
/// `total_amount_cents` is set to 1000 to match the single line item.
async fn seed_open_invoice(pool: &PgPool, business_id: Uuid, customer_id: Uuid) -> Uuid {
    let invoice_id = Uuid::new_v4();

    sqlx::query!(
        r#"INSERT INTO invoices (id, business_id, customer_id, state, total_amount_cents, due_date)
           VALUES ($1, $2, $3, 'Open', 1000, NOW() + interval '30 days')"#,
        invoice_id,
        business_id,
        customer_id,
    )
    .execute(pool)
    .await
    .unwrap();

    sqlx::query!(
        r#"INSERT INTO invoice_line_items
               (id, invoice_id, description, quantity, unit_amount_cents)
           VALUES ($1, $2, 'Test item', 1, 1000)"#,
        Uuid::new_v4(),
        invoice_id,
    )
    .execute(pool)
    .await
    .unwrap();

    invoice_id
}

// ── Mock PSP factories ────────────────────────────────────────────────────────

/// Start a mock PSP that always succeeds after ~50 ms. Returns base URL.
async fn start_psp_success() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(
        HttpServer::new(|| {
            App::new().route(
                "/charge",
                web::post().to(|| async {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    actix_web::HttpResponse::Ok().json(serde_json::json!({
                        "status": "succeeded",
                        "psp_ref": Uuid::new_v4().to_string()
                    }))
                }),
            )
        })
        .listen(listener)
        .unwrap()
        .run(),
    );
    format!("http://127.0.0.1:{}", port)
}

/// Start a mock PSP that succeeds but counts every `/charge` call.
async fn start_psp_counting() -> (String, Arc<AtomicUsize>) {
    let counter = Arc::new(AtomicUsize::new(0));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    {
        let c = counter.clone();
        tokio::spawn(
            HttpServer::new(move || {
                let c2 = c.clone();
                App::new()
                    .app_data(Data::new(c2))
                    .route(
                        "/charge",
                        web::post().to(|cnt: Data<Arc<AtomicUsize>>| async move {
                            cnt.fetch_add(1, Ordering::SeqCst);
                            tokio::time::sleep(Duration::from_millis(50)).await;
                            actix_web::HttpResponse::Ok().json(serde_json::json!({
                                "status": "succeeded",
                                "psp_ref": Uuid::new_v4().to_string()
                            }))
                        }),
                    )
            })
            .listen(listener)
            .unwrap()
            .run(),
        );
    }
    (format!("http://127.0.0.1:{}", port), counter)
}

/// Start a mock PSP that sleeps 35 s — longer than billing's 10 s client timeout.
/// Models `tok_timeout` behaviour.
async fn start_psp_timeout() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(
        HttpServer::new(|| {
            App::new().route(
                "/charge",
                web::post().to(|| async {
                    tokio::time::sleep(Duration::from_secs(35)).await;
                    actix_web::HttpResponse::Ok().json(serde_json::json!({
                        "status": "succeeded",
                        "psp_ref": Uuid::new_v4().to_string()
                    }))
                }),
            )
        })
        .listen(listener)
        .unwrap()
        .run(),
    );
    format!("http://127.0.0.1:{}", port)
}

/// Start a mock PSP that returns HTTP 500 immediately. Models `tok_network_error`.
async fn start_psp_error() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(
        HttpServer::new(|| {
            App::new().route(
                "/charge",
                web::post().to(|| async {
                    actix_web::HttpResponse::InternalServerError()
                        .json(serde_json::json!({"error": "gateway_error"}))
                }),
            )
        })
        .listen(listener)
        .unwrap()
        .run(),
    );
    format!("http://127.0.0.1:{}", port)
}

/// Start the full billing HTTP server. Returns its base URL.
///
/// A 100 ms settle delay lets the OS finish binding before clients connect.
async fn start_billing(pool: PgPool, psp_url: String) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(
        HttpServer::new(move || {
            App::new()
                .app_data(Data::new(AppState {
                    db: pool.clone(),
                    psp_url: psp_url.clone(),
                }))
                .configure(configure_routes)
        })
        .workers(2) // enough concurrency for tests
        .listen(listener)
        .unwrap()
        .run(),
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    format!("http://127.0.0.1:{}", port)
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 1 — Concurrency
// ─────────────────────────────────────────────────────────────────────────────

/// Fire N concurrent `POST /pay` requests for the **same** invoice, each with
/// a distinct `Idempotency-Key` (simulating N different callers racing to pay).
///
/// The billing service uses an atomic `UPDATE … WHERE state = 'Open'` to
/// transition the invoice to `Processing`. Only one concurrent caller can win
/// that race; the rest must receive `422 Unprocessable Entity`.
///
/// Verified invariants
/// -------------------
/// * Exactly **1** response is `200 OK`.
/// * Every other response is `422` (not 5xx, not a duplicate 200).
/// * Final invoice state is `Paid`.
/// * Exactly **1** `payment_attempts` row has `status = 'Succeeded'`.
#[sqlx::test(migrations = "../migrations")]
async fn test_concurrent_pay_only_one_succeeds(pool: PgPool) {
    let psp_url = start_psp_success().await;
    let (business_id, api_key) = seed_business(&pool).await;
    let customer_id = seed_customer(&pool, business_id).await;
    let invoice_id = seed_open_invoice(&pool, business_id, customer_id).await;
    let billing_url = start_billing(pool.clone(), psp_url).await;

    const N: usize = 10;
    let http = reqwest::Client::new();

    // Build all N futures before polling any, to maximise overlap.
    let futs: Vec<_> = (0..N)
        .map(|i| {
            http.post(format!("{}/api/invoices/{}/pay", billing_url, invoice_id))
                .header("Authorization", format!("Bearer {}", api_key))
                // Different keys → different logical requests, no idempotency short-circuit.
                .header("Idempotency-Key", format!("race-{}-{}", invoice_id, i))
                .json(&serde_json::json!({"card_token": "tok_success"}))
                .send()
        })
        .collect();

    let responses = join_all(futs).await;

    let mut success = 0usize;
    let mut unprocessable = 0usize;
    for r in responses {
        match r.expect("HTTP request failed").status().as_u16() {
            200 => success += 1,
            422 => unprocessable += 1,
            other => panic!("unexpected HTTP status {other} from concurrent /pay"),
        }
    }

    assert_eq!(success, 1, "exactly one concurrent payment must succeed");
    assert_eq!(
        success + unprocessable,
        N,
        "all {N} requests must be 200 or 422 — none may 5xx or be silently lost"
    );

    // DB: invoice must be Paid
    let inv = sqlx::query!("SELECT state FROM invoices WHERE id = $1", invoice_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(inv.state, "Paid", "invoice must be Paid after the race");

    // DB: exactly one Succeeded attempt — no double-charge
    let succeeded: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM payment_attempts \
         WHERE invoice_id = $1 AND status = 'Succeeded'",
        invoice_id
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .unwrap_or(0);

    assert_eq!(succeeded, 1, "exactly one Succeeded attempt must exist, got {succeeded}");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 2 — Idempotency
// ─────────────────────────────────────────────────────────────────────────────

/// Replaying the **same** `Idempotency-Key` must return an identical response
/// body without triggering a second PSP call.
///
/// Verified invariants
/// -------------------
/// * Second response body == first response body (byte-for-byte JSON equality).
/// * PSP `/charge` endpoint was called **exactly once** across both HTTP calls.
/// * Exactly **1** row exists in `payment_attempts`.
#[sqlx::test(migrations = "../migrations")]
async fn test_idempotent_pay_replays_without_second_psp_call(pool: PgPool) {
    let (psp_url, call_count) = start_psp_counting().await;
    let (business_id, api_key) = seed_business(&pool).await;
    let customer_id = seed_customer(&pool, business_id).await;
    let invoice_id = seed_open_invoice(&pool, business_id, customer_id).await;
    let billing_url = start_billing(pool.clone(), psp_url).await;

    let idem_key = format!("idem-{}", Uuid::new_v4());
    let http = reqwest::Client::new();

    let build_req = || {
        http.post(format!("{}/api/invoices/{}/pay", billing_url, invoice_id))
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Idempotency-Key", idem_key.clone())
            .json(&serde_json::json!({"card_token": "tok_success"}))
    };

    // ── First request ──────────────────────────────────────────────────────
    let resp1 = build_req().send().await.unwrap();
    assert_eq!(resp1.status().as_u16(), 200, "first request must succeed");
    let body1: serde_json::Value = resp1.json().await.unwrap();

    // ── Second request (replay) ────────────────────────────────────────────
    let resp2 = build_req().send().await.unwrap();
    assert_eq!(resp2.status().as_u16(), 200, "replay must return 200");
    let body2: serde_json::Value = resp2.json().await.unwrap();

    assert_eq!(body1, body2, "idempotent replay must return an identical response body");

    // PSP was called once — the second request must be served from the cache.
    assert_eq!(
        call_count.load(Ordering::SeqCst),
        1,
        "PSP must be called exactly once; idempotency replay must not reach PSP"
    );

    // Only one payment attempt in the DB.
    let attempts: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM payment_attempts WHERE invoice_id = $1",
        invoice_id
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .unwrap_or(0);

    assert_eq!(attempts, 1, "exactly one payment_attempt must exist after replay");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 3a — PSP timeout
// ─────────────────────────────────────────────────────────────────────────────

/// When the PSP does not respond within billing's **10-second** client timeout
/// the invoice must not be left in `Processing` — it must roll back to `Open`
/// so the merchant can retry.
///
/// This test completes in ~10 s (billing's PSP timeout), not 35 s (PSP sleep).
///
/// Verified invariants
/// -------------------
/// * API returns `402 Payment Required` — not a hang, not a 5xx.
/// * Invoice state is `Open` — **not** stuck in `Processing`.
/// * The `payment_attempts` row has `status = 'TimedOut'`.
#[sqlx::test(migrations = "../migrations")]
async fn test_psp_timeout_invoice_not_stuck_in_processing(pool: PgPool) {
    let psp_url = start_psp_timeout().await;
    let (business_id, api_key) = seed_business(&pool).await;
    let customer_id = seed_customer(&pool, business_id).await;
    let invoice_id = seed_open_invoice(&pool, business_id, customer_id).await;
    let billing_url = start_billing(pool.clone(), psp_url).await;

    // Test-level timeout is longer than billing's 10 s PSP timeout.
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap();

    let resp = http
        .post(format!("{}/api/invoices/{}/pay", billing_url, invoice_id))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Idempotency-Key", format!("timeout-{}", Uuid::new_v4()))
        .json(&serde_json::json!({"card_token": "tok_timeout"}))
        .send()
        .await
        .expect("test HTTP client must not time out before billing does");

    assert_eq!(
        resp.status().as_u16(),
        402,
        "PSP timeout must yield 402 Payment Required"
    );

    // Invoice must NOT be stuck in Processing.
    let inv = sqlx::query!("SELECT state FROM invoices WHERE id = $1", invoice_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        inv.state, "Open",
        "invoice must return to Open after PSP timeout — found '{}' instead",
        inv.state
    );

    // Attempt must be recorded as TimedOut.
    let attempt = sqlx::query!(
        "SELECT status FROM payment_attempts \
         WHERE invoice_id = $1 ORDER BY created_at DESC LIMIT 1",
        invoice_id
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(attempt.status, "TimedOut", "attempt must be marked TimedOut");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 3b — PSP network error  (HTTP 500 / gateway failure)
// ─────────────────────────────────────────────────────────────────────────────

/// When the PSP returns HTTP 500 the invoice must roll back to `Open`.
///
/// Verified invariants
/// -------------------
/// * API returns `402`.
/// * Invoice state is `Open`.
/// * The `payment_attempts` row has `status = 'Error'`.
#[sqlx::test(migrations = "../migrations")]
async fn test_psp_network_error_invoice_returns_to_open(pool: PgPool) {
    let psp_url = start_psp_error().await;
    let (business_id, api_key) = seed_business(&pool).await;
    let customer_id = seed_customer(&pool, business_id).await;
    let invoice_id = seed_open_invoice(&pool, business_id, customer_id).await;
    let billing_url = start_billing(pool.clone(), psp_url).await;

    let http = reqwest::Client::new();
    let resp = http
        .post(format!("{}/api/invoices/{}/pay", billing_url, invoice_id))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Idempotency-Key", format!("neterr-{}", Uuid::new_v4()))
        .json(&serde_json::json!({"card_token": "tok_network_error"}))
        .send()
        .await
        .unwrap();

    assert_eq!(
        resp.status().as_u16(),
        402,
        "PSP 500 must yield 402 Payment Required"
    );

    let inv = sqlx::query!("SELECT state FROM invoices WHERE id = $1", invoice_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        inv.state, "Open",
        "invoice must return to Open after gateway error"
    );

    let attempt = sqlx::query!(
        "SELECT status FROM payment_attempts \
         WHERE invoice_id = $1 ORDER BY created_at DESC LIMIT 1",
        invoice_id
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(attempt.status, "Error", "attempt must be marked Error");
}
