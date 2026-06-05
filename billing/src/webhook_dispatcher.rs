use std::{sync::Arc, time::Duration};

use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::PgPool;
use tokio::time::sleep;

use crate::repositories::webhook_repository::WebhookRepository;

type HmacSha256 = Hmac<Sha256>;

fn sign_payload(secret: &str, body: &[u8]) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(body);
    let result = mac.finalize();
    format!("sha256={}", hex::encode(result.into_bytes()))
}

/// `pool`           — shared database pool
/// `poll_interval`  — how often to check for due deliveries (e.g. 5 seconds)
pub fn spawn(pool: Arc<PgPool>, poll_interval: Duration) {
    actix_web::rt::spawn(async move {
        log::info!(
            "Webhook dispatcher started (poll interval: {}s)",
            poll_interval.as_secs()
        );
        run(pool, poll_interval).await;
    });
}

async fn run(pool: Arc<PgPool>, poll_interval: Duration) {
    let repo = WebhookRepository;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("Failed to build reqwest client for webhook dispatcher");

    loop {
        match repo.find_due_deliveries(&pool, 50).await {
            Ok(deliveries) => {
                if !deliveries.is_empty() {
                    log::debug!("Webhook dispatcher: {} deliveries due", deliveries.len());
                }

                for delivery in deliveries {
                    let endpoint = match sqlx::query!(
                        r#"
                        SELECT url, secret
                        FROM webhook_endpoints
                        WHERE id = $1 AND is_active = true
                        "#,
                        delivery.endpoint_id,
                    )
                    .fetch_optional(pool.as_ref())
                    .await
                    {
                        Ok(Some(row)) => row,
                        Ok(None) => {
                            log::info!(
                                "Webhook endpoint {} is inactive; cancelling delivery {}",
                                delivery.endpoint_id,
                                delivery.id
                            );
                            let _ = repo
                                .mark_failed_or_retry(&pool, delivery.id, 6, "endpoint deactivated")
                                .await;
                            continue;
                        }
                        Err(e) => {
                            log::error!("DB error looking up endpoint: {e}");
                            continue;
                        }
                    };

                    let body_bytes = match serde_json::to_vec(&delivery.payload) {
                        Ok(b) => b,
                        Err(e) => {
                            log::error!(
                                "Failed to serialise webhook payload for delivery {}: {e}",
                                delivery.id
                            );
                            let _ = repo
                                .mark_failed_or_retry(
                                    &pool,
                                    delivery.id,
                                    delivery.attempt_count,
                                    &e.to_string(),
                                )
                                .await;
                            continue;
                        }
                    };

                    let signature = sign_payload(&endpoint.secret, &body_bytes);

                    log::debug!(
                        "Delivering {} to {} (attempt {})",
                        delivery.event_type,
                        endpoint.url,
                        delivery.attempt_count + 1,
                    );

                    let result = client
                        .post(&endpoint.url)
                        .header("Content-Type", "application/json")
                        .header("X-Webhook-Event", &delivery.event_type)
                        .header("X-Webhook-Delivery-Id", delivery.id.to_string())
                        .header("X-Webhook-Signature", &signature)
                        .body(body_bytes)
                        .send()
                        .await;

                    match result {
                        Ok(resp) if resp.status().is_success() => {
                            log::info!(
                                "Webhook delivery {} succeeded ({})",
                                delivery.id,
                                resp.status()
                            );
                            let _ = repo.mark_delivered(&pool, delivery.id).await;
                        }
                        Ok(resp) => {
                            let err = format!("HTTP {}", resp.status());
                            log::warn!("Webhook delivery {} failed: {}", delivery.id, err);
                            let _ = repo
                                .mark_failed_or_retry(
                                    &pool,
                                    delivery.id,
                                    delivery.attempt_count,
                                    &err,
                                )
                                .await;
                        }
                        Err(e) => {
                            let err = e.to_string();
                            log::warn!("Webhook delivery {} network error: {}", delivery.id, err);
                            let _ = repo
                                .mark_failed_or_retry(
                                    &pool,
                                    delivery.id,
                                    delivery.attempt_count,
                                    &err,
                                )
                                .await;
                        }
                    }
                }
            }
            Err(e) => {
                log::error!("Webhook dispatcher: DB error fetching due deliveries: {e}");
            }
        }

        sleep(poll_interval).await;
    }
}

pub async fn enqueue(
    pool: &PgPool,
    business_id: uuid::Uuid,
    event_type: &str,
    invoice_id: Option<uuid::Uuid>,
    payload: serde_json::Value,
) {
    let repo = WebhookRepository;

    let endpoints = match repo.list_active_by_business(pool, business_id).await {
        Ok(ep) => ep,
        Err(e) => {
            log::error!("Failed to list webhook endpoints for enqueue: {e}");
            return;
        }
    };

    for endpoint in endpoints {
        let delivery_id = uuid::Uuid::new_v4();
        if let Err(e) = repo
            .create_delivery(
                pool,
                delivery_id,
                endpoint.id,
                invoice_id,
                event_type,
                &payload,
            )
            .await
        {
            log::error!(
                "Failed to enqueue webhook delivery for endpoint {}: {e}",
                endpoint.id
            );
        } else {
            log::debug!(
                "Enqueued {} delivery {} for endpoint {}",
                event_type,
                delivery_id,
                endpoint.id
            );
        }
    }
}
