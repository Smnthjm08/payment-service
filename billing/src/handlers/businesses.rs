use actix_web::{Error, HttpResponse, Responder, web};
use serde::Deserialize;
use sha2::Digest;
use uuid::Uuid;

use crate::{
    AppState,
    domains::models::Business,
    repositories::{api_key_repository::ApiKeyRepository, business_repository::BusinessRepository},
};

#[derive(Debug, Deserialize)]
pub struct CreateBusinessRequest {
    pub email: String,
    pub name: String,
}

#[derive(Debug, serde::Serialize)]
pub struct CreateBusinessResponse {
    pub business: Business,
    pub api_key: String,
}

pub fn businesses_scope() -> impl actix_web::dev::HttpServiceFactory {
    web::scope("/businesses").route("", web::post().to(create_business))
}

pub async fn create_business(
    state: web::Data<AppState>,
    payload: web::Json<CreateBusinessRequest>,
) -> Result<impl Responder, Error> {
    let business_repository = BusinessRepository;
    let api_key_repository = ApiKeyRepository;
    let business_id = Uuid::new_v4();
    let api_key_id = Uuid::new_v4();
    let key_prefix = format!("dodo_live_{}", Uuid::new_v4().simple());
    let key_secret = Uuid::new_v4().simple().to_string();
    let api_key = format!("{}.{}", key_prefix, key_secret);
    let key_hash = hex::encode(sha2::Sha256::digest(api_key.as_bytes()));

    let mut tx = state.db.begin().await.map_err(actix_web::error::ErrorInternalServerError)?;

    let business: Business = business_repository
        .create_business(&mut tx, business_id, &payload.email, &payload.name)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    api_key_repository
        .create_api_key(&mut tx, api_key_id, business.id, &key_prefix, &key_hash)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    tx.commit().await.map_err(actix_web::error::ErrorInternalServerError)?;

    Ok(HttpResponse::Created().json(CreateBusinessResponse { business, api_key }))
}
