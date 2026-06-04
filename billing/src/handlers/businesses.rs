use actix_web::{Error, HttpResponse, Responder, web};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    AppState, domains::models::Business, repositories::business_repository::BusinessRepository,
};

#[derive(Debug, Deserialize)]
pub struct CreateBusinessRequest {
    pub email: String,
    pub name: String,
}

pub fn businesses_scope() -> impl actix_web::dev::HttpServiceFactory {
    web::scope("/businesses").route("", web::post().to(create_business))
}

pub async fn create_business(
    state: web::Data<AppState>,
    payload: web::Json<CreateBusinessRequest>,
) -> Result<impl Responder, Error> {
    let business_repository = BusinessRepository;
    let business_id = Uuid::new_v4();

    let business: Business = business_repository
        .create_business(&state.db, business_id, &payload.email, &payload.name)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    Ok(HttpResponse::Created().json(business))
}
