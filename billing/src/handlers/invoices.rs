use actix_web::{Error, HttpMessage, HttpRequest, HttpResponse, Responder, web};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AppState,
    domains::models::{Invoice, InvoiceLineItem, PaymentAttempt},
    middlewares::api_key_middleware::AuthenticatedBusiness,
    psp_client::{self, PspResult},
    repositories::{
        idempotency_key_repository::IdempotencyKeyRepository,
        invoice_repository::InvoiceRepository,
        payment_attempt_repository::PaymentAttemptRepository,
    },
};

#[derive(Debug, Deserialize)]
pub struct CreateInvoiceLineItem {
    pub description: String,
    pub quantity: i32,
    pub unit_amount_cents: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateInvoiceRequest {
    pub customer_id: Uuid,
    pub due_date: DateTime<Utc>,
    pub line_items: Vec<CreateInvoiceLineItem>,
}

#[derive(Debug, Serialize)]
struct InvoiceResponse {
    invoice: Invoice,
    line_items: Vec<InvoiceLineItem>,
}

pub fn invoices_scope() -> impl actix_web::dev::HttpServiceFactory {
    web::scope("/invoices")
        .wrap(crate::middlewares::api_key_middleware::ApiKeyMiddleware)
        .route("", web::post().to(create_invoice))
        .route("", web::get().to(list_invoices))
        .route("/{id}", web::get().to(get_invoice))
        .route("/{id}/pay", web::post().to(pay_invoice))
        .route("/{id}/void", web::post().to(void_invoice))
        .route("/{id}/mark_uncollectible", web::post().to(mark_uncollectible))
}

pub async fn create_invoice(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<CreateInvoiceRequest>,
) -> Result<impl Responder, Error> {
    let auth = req
        .extensions()
        .get::<AuthenticatedBusiness>()
        .cloned()
        .ok_or_else(|| actix_web::error::ErrorUnauthorized("Unauthorized"))?;
    // validate customer exists and belongs to this business
    let customer_repo = crate::repositories::customer_repository::CustomerRepository;
    let customer = customer_repo
        .find_by_id(&state.db, payload.customer_id, auth.business_id)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    if customer.is_none() {
        return Err(actix_web::error::ErrorBadRequest("Invalid customer_id for this business"));
    }

    // validate line items
    if payload.line_items.is_empty() {
        return Err(actix_web::error::ErrorBadRequest("line_items cannot be empty"));
    }

    for li in &payload.line_items {
        if li.description.trim().is_empty() {
            return Err(actix_web::error::ErrorBadRequest("line item description cannot be empty"));
        }
        if li.quantity <= 0 {
            return Err(actix_web::error::ErrorBadRequest("line item quantity must be > 0"));
        }
        if li.unit_amount_cents < 0 {
            return Err(actix_web::error::ErrorBadRequest("unit_amount_cents must be >= 0"));
        }
    }

    let repo = InvoiceRepository;
    let invoice_id = Uuid::new_v4();

    let items_input: Vec<(Uuid, String, i32, i64)> = payload
        .line_items
        .iter()
        .map(|li| (Uuid::new_v4(), li.description.clone(), li.quantity, li.unit_amount_cents))
        .collect();

    let (invoice, items) = repo
        .create_invoice(
            &state.db,
            invoice_id,
            auth.business_id,
            payload.customer_id,
            payload.due_date,
            items_input,
        )
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    Ok(HttpResponse::Created().json(InvoiceResponse { invoice, line_items: items }))
}

#[derive(Debug, Deserialize)]
pub struct ListInvoicesQuery {
    pub state: Option<String>,
}

pub async fn list_invoices(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<ListInvoicesQuery>,
) -> Result<impl Responder, Error> {
    let auth = req
        .extensions()
        .get::<AuthenticatedBusiness>()
        .cloned()
        .ok_or_else(|| actix_web::error::ErrorUnauthorized("Unauthorized"))?;

    let repo = InvoiceRepository;

    let state_filter = query.state.as_deref();

    let rows = repo
        .list_by_business(&state.db, auth.business_id, state_filter)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    let resp: Vec<InvoiceResponse> = rows
        .into_iter()
        .map(|(invoice, line_items)| InvoiceResponse { invoice, line_items })
        .collect();

    Ok(HttpResponse::Ok().json(resp))
}

pub async fn get_invoice(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
) -> Result<impl Responder, Error> {
    let auth = req
        .extensions()
        .get::<AuthenticatedBusiness>()
        .cloned()
        .ok_or_else(|| actix_web::error::ErrorUnauthorized("Unauthorized"))?;

    let repo = InvoiceRepository;

    let maybe = repo
        .find_by_id(&state.db, path.into_inner(), auth.business_id)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    match maybe {
        Some((invoice, line_items)) => Ok(HttpResponse::Ok().json(InvoiceResponse { invoice, line_items })),
        None => Ok(HttpResponse::NotFound().finish()),
    }
}

#[derive(Debug, Deserialize)]
pub struct PayInvoiceRequest {
    pub card_token: String,
}

#[derive(Debug, Serialize)]
pub struct PayInvoiceResponse {
    pub invoice: Invoice,
    pub payment_attempt: PaymentAttemptView,
}


#[derive(Debug, Serialize)]
pub struct PaymentAttemptView {
    pub id: Uuid,
    pub invoice_id: Uuid,
    pub status: String,
    pub amount_cents: i64,
    pub card_token: String,
    pub psp_ref: Option<String>,
    pub failure_code: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<PaymentAttempt> for PaymentAttemptView {
    fn from(a: PaymentAttempt) -> Self {
        use crate::domains::enums::PaymentAttemptStatus;
        let status = match a.status {
            PaymentAttemptStatus::Pending    => "Pending",
            PaymentAttemptStatus::Succeeded  => "Succeeded",
            PaymentAttemptStatus::Failed     => "Failed",
            PaymentAttemptStatus::TimedOut   => "TimedOut",
            PaymentAttemptStatus::Error      => "Error",
        };
        Self {
            id: a.id,
            invoice_id: a.invoice_id,
            status: status.to_string(),
            amount_cents: a.amount_cents,
            card_token: a.card_token,
            psp_ref: a.psp_ref,
            failure_code: a.failure_code,
            created_at: a.created_at,
            updated_at: a.updated_at,
        }
    }
}

pub async fn pay_invoice(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    payload: web::Json<PayInvoiceRequest>,
) -> Result<impl Responder, Error> {
    let auth = req
        .extensions()
        .get::<AuthenticatedBusiness>()
        .cloned()
        .ok_or_else(|| actix_web::error::ErrorUnauthorized("Unauthorized"))?;

    let invoice_id = path.into_inner();

    let idempotency_key_str = req
        .headers()
        .get("Idempotency-Key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    let idempotency_key_str = match idempotency_key_str {
        Some(k) if !k.trim().is_empty() => k,
        _ => {
            return Err(actix_web::error::ErrorBadRequest(
                "Idempotency-Key header is required",
            ));
        }
    };

    let idem_repo = IdempotencyKeyRepository;

    if let Some(cached) = idem_repo
        .find_by_key(&state.db, auth.business_id, &idempotency_key_str)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?
    {
        if let (Some(body), Some(code)) = (cached.response_body, cached.status_code) {
            log::info!(
                "Idempotency hit for key={} — replaying cached response",
                idempotency_key_str
            );
            return Ok(HttpResponse::build(
                actix_web::http::StatusCode::from_u16(code as u16)
                    .unwrap_or(actix_web::http::StatusCode::OK),
            )
            .json(body));
        }
        return Err(actix_web::error::ErrorConflict(
            "A payment with this Idempotency-Key is already in progress",
        ));
    }

    let request_hash = {
        use sha2::{Digest, Sha256};
        let input = format!("{}{}", invoice_id, payload.card_token);
        hex::encode(Sha256::digest(input.as_bytes()))
    };

    let idem_record = idem_repo
        .create_key(&state.db, auth.business_id, &idempotency_key_str, &request_hash)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    let invoice_repo = InvoiceRepository;
    let attempt_repo = PaymentAttemptRepository;

    let mut tx = state
        .db
        .begin()
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    let invoice = invoice_repo
        .transition_to_processing(&mut tx, invoice_id, auth.business_id)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?
        .ok_or_else(|| {
            actix_web::error::ErrorUnprocessableEntity(
                "Invoice not found or is not in the Open state",
            )
        })?;

    let attempt_id = Uuid::new_v4();
    let _attempt = attempt_repo
        .create_attempt(
            &mut tx,
            attempt_id,
            invoice_id,
            invoice.total_amount_cents,
            &payload.card_token,
        )
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    tx.commit()
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    log::info!(
        "Invoice {} moved to Processing; attempt {} created. Calling PSP…",
        invoice_id,
        attempt_id
    );

    let psp_result = psp_client::charge(
        &state.psp_url,
        &payload.card_token,
        invoice.total_amount_cents,
        attempt_id, // use attempt UUID as PSP-level idempotency key
    )
    .await;

    // ── 10–13. Record outcome and update invoice state ────────────────────────
    let (final_attempt, final_invoice) = match psp_result {
        PspResult::Success { psp_ref, raw } => {
            log::info!("PSP success for attempt {}; psp_ref={}", attempt_id, psp_ref);
            let updated_attempt = attempt_repo
                .update_attempt(
                    &state.db,
                    attempt_id,
                    "Succeeded",
                    Some(&psp_ref),
                    Some(raw),
                    None,
                )
                .await
                .map_err(actix_web::error::ErrorInternalServerError)?;

            let paid_invoice = invoice_repo
                .transition_to_paid(&state.db, invoice_id)
                .await
                .map_err(actix_web::error::ErrorInternalServerError)?;

            (updated_attempt, paid_invoice)
        }

        PspResult::Declined { failure_code, raw } => {
            log::warn!(
                "PSP declined attempt {}; failure_code={}",
                attempt_id,
                failure_code
            );
            let updated_attempt = attempt_repo
                .update_attempt(
                    &state.db,
                    attempt_id,
                    "Failed",
                    None,
                    Some(raw),
                    Some(&failure_code),
                )
                .await
                .map_err(actix_web::error::ErrorInternalServerError)?;

            let open_invoice = invoice_repo
                .transition_to_open(&state.db, invoice_id)
                .await
                .map_err(actix_web::error::ErrorInternalServerError)?;

            (updated_attempt, open_invoice)
        }

        PspResult::TimedOut => {
            log::warn!("PSP timed out for attempt {}", attempt_id);
            let updated_attempt = attempt_repo
                .update_attempt(
                    &state.db,
                    attempt_id,
                    "TimedOut",
                    None,
                    Some(serde_json::json!({ "error": "psp_timeout" })),
                    Some("psp_timeout"),
                )
                .await
                .map_err(actix_web::error::ErrorInternalServerError)?;

            let open_invoice = invoice_repo
                .transition_to_open(&state.db, invoice_id)
                .await
                .map_err(actix_web::error::ErrorInternalServerError)?;

            (updated_attempt, open_invoice)
        }

        PspResult::GatewayError { raw } => {
            log::error!("PSP gateway error for attempt {}: {:?}", attempt_id, raw);
            let updated_attempt = attempt_repo
                .update_attempt(
                    &state.db,
                    attempt_id,
                    "Error",
                    None,
                    Some(raw),
                    Some("gateway_error"),
                )
                .await
                .map_err(actix_web::error::ErrorInternalServerError)?;

            let open_invoice = invoice_repo
                .transition_to_open(&state.db, invoice_id)
                .await
                .map_err(actix_web::error::ErrorInternalServerError)?;

            (updated_attempt, open_invoice)
        }
    };

    let status_code: u16 = match final_attempt.status {
        crate::domains::enums::PaymentAttemptStatus::Succeeded => 200,
        _ => 402,
    };

    let response_body = PayInvoiceResponse {
        invoice: final_invoice,
        payment_attempt: final_attempt.into(),
    };

    let json_value = serde_json::to_value(&response_body)
        .map_err(actix_web::error::ErrorInternalServerError)?;

    if let Err(e) = idem_repo
        .complete_key(&state.db, idem_record.id, json_value.clone(), status_code as i32)
        .await
    {
        log::error!("Failed to complete idempotency key record: {e}");
    }

    Ok(HttpResponse::build(
        actix_web::http::StatusCode::from_u16(status_code)
            .unwrap_or(actix_web::http::StatusCode::OK),
    )
    .json(json_value))
}

pub async fn void_invoice(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
) -> Result<impl Responder, Error> {
    let auth = req
        .extensions()
        .get::<AuthenticatedBusiness>()
        .cloned()
        .ok_or_else(|| actix_web::error::ErrorUnauthorized("Unauthorized"))?;

    let invoice_id = path.into_inner();
    let invoice_repo = InvoiceRepository;

    match invoice_repo
        .transition_to_void(&state.db, invoice_id, auth.business_id)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?
    {
        Some(invoice) => Ok(HttpResponse::Ok().json(invoice)),
        None => {
            let maybe = invoice_repo
                .find_by_id(&state.db, invoice_id, auth.business_id)
                .await
                .map_err(actix_web::error::ErrorInternalServerError)?;

            match maybe {
                None => Err(actix_web::error::ErrorNotFound("Invoice not found")),
                Some((inv, _)) => Err(actix_web::error::ErrorUnprocessableEntity(format!(
                    "Cannot void an invoice in '{}' state. \
                     Only invoices in 'Open' state can be voided.",
                    inv.state
                ))),
            }
        }
    }
}

pub async fn mark_uncollectible(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
) -> Result<impl Responder, Error> {
    let auth = req
        .extensions()
        .get::<AuthenticatedBusiness>()
        .cloned()
        .ok_or_else(|| actix_web::error::ErrorUnauthorized("Unauthorized"))?;

    let invoice_id = path.into_inner();
    let invoice_repo = InvoiceRepository;

    match invoice_repo
        .transition_to_uncollectible(&state.db, invoice_id, auth.business_id)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?
    {
        Some(invoice) => Ok(HttpResponse::Ok().json(invoice)),
        None => {
            let maybe = invoice_repo
                .find_by_id(&state.db, invoice_id, auth.business_id)
                .await
                .map_err(actix_web::error::ErrorInternalServerError)?;

            match maybe {
                None => Err(actix_web::error::ErrorNotFound("Invoice not found")),
                Some((inv, _)) => Err(actix_web::error::ErrorUnprocessableEntity(format!(
                    "Cannot mark an invoice in '{}' state as uncollectible. \
                     Only invoices in 'Open' state can be marked uncollectible.",
                    inv.state
                ))),
            }
        }
    }
}
