use actix_web::{Error, HttpMessage, HttpRequest, HttpResponse, Responder, web};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AppState,
    domains::models::{Invoice, InvoiceLineItem},
    middlewares::api_key_middleware::AuthenticatedBusiness,
    repositories::invoice_repository::InvoiceRepository,
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
