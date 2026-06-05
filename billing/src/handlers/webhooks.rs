use actix_web::{Error, HttpMessage, HttpRequest, HttpResponse, Responder, web};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AppState, middlewares::api_key_middleware::AuthenticatedBusiness,
    repositories::webhook_repository::WebhookRepository,
};

fn gen_secret() -> String {
    // 32 random bytes → hex = 64 char signing secret
    let bytes: Vec<u8> = (0..32).map(|_| rand::random::<u8>()).collect();
    hex::encode(bytes)
}

#[derive(Debug, Deserialize)]
pub struct CreateWebhookRequest {
    pub url: String,
}

#[derive(Debug, Serialize)]
pub struct CreateWebhookResponse {
    pub id: Uuid,
    pub business_id: Uuid,
    pub url: String,
    pub is_active: bool,
    /// Returned only on creation
    pub secret: String,
}

/// List / get response — secret is always redacted.
#[derive(Debug, Serialize)]
pub struct WebhookEndpointView {
    pub id: Uuid,
    pub business_id: Uuid,
    pub url: String,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub fn webhooks_scope() -> impl actix_web::dev::HttpServiceFactory {
    web::scope("/webhooks")
        .wrap(crate::middlewares::api_key_middleware::ApiKeyMiddleware)
        .route("", web::post().to(create_webhook))
        .route("", web::get().to(list_webhooks))
        .route("/{id}", web::delete().to(delete_webhook))
}

pub async fn create_webhook(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<CreateWebhookRequest>,
) -> Result<impl Responder, Error> {
    let auth = auth_business(&req)?;

    if payload.url.trim().is_empty() {
        return Err(actix_web::error::ErrorBadRequest("url is required"));
    }
    if !payload.url.starts_with("http://") && !payload.url.starts_with("https://") {
        return Err(actix_web::error::ErrorBadRequest(
            "url must start with http:// or https://",
        ));
    }

    let secret = gen_secret();
    let id = Uuid::new_v4();

    let repo = WebhookRepository;
    let endpoint = repo
        .create_endpoint(&state.db, id, auth.business_id, &payload.url, &secret)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    Ok(HttpResponse::Created().json(CreateWebhookResponse {
        id: endpoint.id,
        business_id: endpoint.business_id,
        url: endpoint.url,
        is_active: endpoint.is_active,
        secret, // shown once
    }))
}

/// `GET /api/webhooks`
pub async fn list_webhooks(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> Result<impl Responder, Error> {
    let auth = auth_business(&req)?;
    let repo = WebhookRepository;

    let endpoints = repo
        .list_by_business(&state.db, auth.business_id)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    let views: Vec<WebhookEndpointView> = endpoints
        .into_iter()
        .map(|e| WebhookEndpointView {
            id: e.id,
            business_id: e.business_id,
            url: e.url,
            is_active: e.is_active,
            created_at: e.created_at,
        })
        .collect();

    Ok(HttpResponse::Ok().json(views))
}

// `DELETE /api/webhooks/{id}`
pub async fn delete_webhook(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
) -> Result<impl Responder, Error> {
    let auth = auth_business(&req)?;
    let repo = WebhookRepository;
    let id = path.into_inner();

    match repo
        .deactivate(&state.db, id, auth.business_id)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?
    {
        Some(e) => Ok(HttpResponse::Ok().json(WebhookEndpointView {
            id: e.id,
            business_id: e.business_id,
            url: e.url,
            is_active: e.is_active,
            created_at: e.created_at,
        })),
        None => Err(actix_web::error::ErrorNotFound(
            "Webhook endpoint not found",
        )),
    }
}

fn auth_business(req: &HttpRequest) -> Result<AuthenticatedBusiness, Error> {
    req.extensions()
        .get::<AuthenticatedBusiness>()
        .cloned()
        .ok_or_else(|| actix_web::error::ErrorUnauthorized("Unauthorized"))
}
