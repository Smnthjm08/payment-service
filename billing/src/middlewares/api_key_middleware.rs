use std::{future::Future, future::Ready, pin::Pin, rc::Rc};

use actix_web::{
    Error, HttpMessage, HttpResponse,
    body::{EitherBody, MessageBody},
    dev::{Service, ServiceRequest, ServiceResponse, Transform, forward_ready},
    http::header::AUTHORIZATION,
    web::Data,
};
use hex::encode as hex_encode;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{AppState, repositories::api_key_repository::ApiKeyRepository};

#[derive(Debug, Clone)]
pub struct AuthenticatedBusiness {
    pub business_id: Uuid,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ApiKeyMiddleware;

pub struct ApiKeyMiddlewareService<S> {
    service: Rc<S>,
}

impl<S, B> Transform<S, ServiceRequest> for ApiKeyMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type InitError = ();
    type Transform = ApiKeyMiddlewareService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        std::future::ready(Ok(ApiKeyMiddlewareService {
            service: Rc::new(service),
        }))
    }
}

impl<S, B> Service<ServiceRequest> for ApiKeyMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let service = self.service.clone();

        Box::pin(async move {
            let auth_business = match authenticate_business(&req).await {
                Ok(auth_business) => auth_business,
                Err(response) => return Ok(req.into_response(response.map_into_right_body())),
            };

            req.extensions_mut().insert(auth_business);

            let response = service.call(req).await?.map_into_left_body();
            Ok(response)
        })
    }
}

async fn authenticate_business(
    req: &ServiceRequest,
) -> Result<AuthenticatedBusiness, HttpResponse> {
    let api_key = extract_api_key(req)?;
    let key_prefix = api_key
        .split_once('.')
        .map(|(prefix, _)| prefix)
        .ok_or_else(unauthorized)?;

    let pool = pool_from_request(req)?;
    let api_key_repository = ApiKeyRepository;

    let stored_key = api_key_repository
        .find_by_prefix(pool, key_prefix)
        .await
        .map_err(|_| unauthorized())?
        .ok_or_else(unauthorized)?;

    let incoming_hash = hex_encode(Sha256::digest(api_key.as_bytes()));
    if incoming_hash != stored_key.key_hash {
        return Err(unauthorized());
    }

    Ok(AuthenticatedBusiness {
        business_id: stored_key.business_id,
    })
}

fn extract_api_key(req: &ServiceRequest) -> Result<String, HttpResponse> {
    let header_value = req
        .headers()
        .get(AUTHORIZATION)
        .ok_or_else(unauthorized)?
        .to_str()
        .map_err(|_| unauthorized())?;

    header_value
        .strip_prefix("Bearer ")
        .map(str::to_owned)
        .ok_or_else(unauthorized)
}

fn pool_from_request(req: &ServiceRequest) -> Result<&PgPool, HttpResponse> {
    let app_state = req.app_data::<Data<AppState>>().ok_or_else(unauthorized)?;

    Ok(&app_state.db)
}

fn unauthorized() -> HttpResponse {
    HttpResponse::Unauthorized().finish()
}
