use actix_web::{Error, HttpMessage, HttpRequest, HttpResponse, Responder, web};
use serde::Serialize;
use sha2::Digest;
use uuid::Uuid;

use crate::{
    AppState,
    domains::models::ApiKey,
    middlewares::api_key_middleware::{ApiKeyMiddleware, AuthenticatedBusiness},
    repositories::api_key_repository::ApiKeyRepository,
};

#[derive(Debug, Serialize)]
struct ApiKeyListResponse {
    api_keys: Vec<ApiKey>,
}

#[derive(Debug, Serialize)]
struct CreateApiKeyResponse {
    api_key: String,
    api_key_record: ApiKey,
}

pub fn api_keys_scope() -> impl actix_web::dev::HttpServiceFactory {
    web::scope("/api-keys")
        .wrap(ApiKeyMiddleware)
        .route("", web::post().to(create_api_key))
        .route("", web::get().to(list_api_keys))
        .route("/{id}", web::get().to(get_api_key))
}

pub async fn create_api_key(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> Result<impl Responder, Error> {
    let auth = authenticated_business(&req)?;
    let api_key_repository = ApiKeyRepository;
    let api_key_id = Uuid::new_v4();
    let prefix = format!("dodo_live_{}", Uuid::new_v4().simple());
    let secret = Uuid::new_v4().simple().to_string();
    let api_key = format!("{}.{}", prefix, secret);
    let key_hash = hex::encode(sha2::Sha256::digest(api_key.as_bytes()));

    let api_key_record = api_key_repository
        .create_api_key(&state.db, api_key_id, auth.business_id, &prefix, &key_hash)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    Ok(HttpResponse::Created().json(CreateApiKeyResponse {
        api_key,
        api_key_record,
    }))
}

pub async fn list_api_keys(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> Result<impl Responder, Error> {
    let auth = authenticated_business(&req)?;
    let api_key_repository = ApiKeyRepository;

    let api_keys = api_key_repository
        .list_by_business(&state.db, auth.business_id)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    Ok(HttpResponse::Ok().json(ApiKeyListResponse { api_keys }))
}

pub async fn get_api_key(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
) -> Result<impl Responder, Error> {
    let auth = authenticated_business(&req)?;
    let api_key_repository = ApiKeyRepository;

    let api_key = api_key_repository
        .find_by_id(&state.db, path.into_inner(), auth.business_id)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    match api_key {
        Some(api_key) => Ok(HttpResponse::Ok().json(api_key)),
        None => Ok(HttpResponse::NotFound().finish()),
    }
}

fn authenticated_business(req: &HttpRequest) -> Result<AuthenticatedBusiness, Error> {
    req.extensions()
        .get::<AuthenticatedBusiness>()
        .cloned()
        .ok_or_else(|| actix_web::error::ErrorUnauthorized("Unauthorized"))
}
