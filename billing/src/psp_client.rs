use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug)]
pub enum PspResult {
    // The PSP accepted and authorised.
    Success { psp_ref: String, raw: serde_json::Value },
    // insufficient_funds, card_declined.
    Declined { failure_code: String, raw: serde_json::Value },
    /// The PSP returned a non-2xx status code.
    GatewayError { raw: serde_json::Value },
    // PSP exceeded the configured timeout.
    TimedOut,
}

#[derive(Debug, Serialize)]
struct ChargeRequest {
    card_token: String,
    amount_cents: i64,
    idempotency_key: String,
}


#[derive(Debug, Deserialize)]
struct ChargeResponseBody {
    status: String,
    // psp_ref
    psp_ref: Option<String>,
    // failure code
    code: Option<String>,
}


pub async fn charge(
    psp_base_url: &str,
    card_token: &str,
    amount_cents: i64,
    idempotency_key: Uuid,
) -> PspResult {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to build reqwest client: {e}");
            return PspResult::GatewayError {
                raw: serde_json::json!({ "error": "client_build_failed" }),
            };
        }
    };

    let url = format!("{}/charge", psp_base_url);
    let body = ChargeRequest {
        card_token: card_token.to_owned(),
        amount_cents,
        idempotency_key: idempotency_key.to_string(),
    };

    let response = match client.post(&url).json(&body).send().await {
        Ok(r) => r,
        Err(e) if e.is_timeout() => {
            log::warn!("PSP request timed out (tok_timeout or slow network): {e}");
            return PspResult::TimedOut;
        }
        Err(e) => {
            log::error!("PSP request failed (tok_network_error or unreachable): {e}");
            return PspResult::GatewayError {
                raw: serde_json::json!({ "error": e.to_string() }),
            };
        }
    };

    let http_status = response.status();

    let raw: serde_json::Value = match response.json().await {
        Ok(v) => v,
        Err(_) => serde_json::json!({}),
    };

    if !http_status.is_success() {
        log::warn!("PSP returned non-2xx status {http_status} (tok_network_error)");
        return PspResult::GatewayError { raw };
    }

    match serde_json::from_value::<ChargeResponseBody>(raw.clone()) {
        Ok(body) if body.status == "succeeded" => PspResult::Success {
            psp_ref: body.psp_ref.unwrap_or_else(|| format!("psp_{}", Uuid::new_v4())),
            raw,
        },
        Ok(body) if body.status == "failed" => PspResult::Declined {
            failure_code: body.code.unwrap_or_else(|| "unknown".to_string()),
            raw,
        },
        Ok(body) => {
            log::error!("PSP returned unrecognised status value: {}", body.status);
            PspResult::GatewayError { raw }
        }
        Err(e) => {
            log::error!("Failed to parse PSP response: {e}");
            PspResult::GatewayError { raw }
        }
    }
}
