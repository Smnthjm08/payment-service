use actix_web::{Error, HttpMessage, HttpRequest, HttpResponse, Responder, web};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AppState,
    domains::models::Customer,
    middlewares::api_key_middleware::{ApiKeyMiddleware, AuthenticatedBusiness},
    repositories::customer_repository::CustomerRepository,
};

#[derive(Debug, Deserialize)]
pub struct CreateCustomerRequest {
    pub name: String,
    pub email: String,
    pub phone: Option<String>,
}

#[derive(Debug, Serialize)]
struct CustomerListResponse {
    customers: Vec<Customer>,
}

pub fn customers_scope() -> impl actix_web::dev::HttpServiceFactory {
    web::scope("/customers")
        .wrap(ApiKeyMiddleware)
        .route("", web::post().to(create_customer))
        .route("", web::get().to(list_customers))
        .route("/{id}", web::get().to(get_customer))
}

pub async fn create_customer(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<CreateCustomerRequest>,
) -> Result<impl Responder, Error> {
    let auth = authenticated_business(&req)?;
    let customer_repository = CustomerRepository;
    let customer_id = Uuid::new_v4();

    let customer = customer_repository
        .create_customer(
            &state.db,
            customer_id,
            auth.business_id,
            &payload.name,
            &payload.email,
            payload.phone.as_deref(),
        )
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    Ok(HttpResponse::Created().json(customer))
}

pub async fn list_customers(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> Result<impl Responder, Error> {
    let auth = authenticated_business(&req)?;
    let customer_repository = CustomerRepository;

    let customers = customer_repository
        .list_by_business(&state.db, auth.business_id)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    Ok(HttpResponse::Ok().json(CustomerListResponse { customers }))
}

pub async fn get_customer(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
) -> Result<impl Responder, Error> {
    let auth = authenticated_business(&req)?;
    let customer_repository = CustomerRepository;

    let customer = customer_repository
        .find_by_id(&state.db, path.into_inner(), auth.business_id)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    match customer {
        Some(customer) => Ok(HttpResponse::Ok().json(customer)),
        None => Ok(HttpResponse::NotFound().finish()),
    }
}

fn authenticated_business(req: &HttpRequest) -> Result<AuthenticatedBusiness, Error> {
    req.extensions()
        .get::<AuthenticatedBusiness>()
        .cloned()
        .ok_or_else(|| actix_web::error::ErrorUnauthorized("Unauthorized"))
}
