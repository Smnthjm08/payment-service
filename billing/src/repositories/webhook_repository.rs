use chrono::{Duration, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domains::models::{WebhookDelivery, WebhookEndpoint};

pub struct WebhookRepository;

impl WebhookRepository {
    pub async fn create_endpoint(
        &self,
        pool: &PgPool,
        id: Uuid,
        business_id: Uuid,
        url: &str,
        secret: &str,
    ) -> Result<WebhookEndpoint, sqlx::Error> {
        sqlx::query_as!(
            WebhookEndpoint,
            r#"
            INSERT INTO webhook_endpoints (id, business_id, url, secret)
            VALUES ($1, $2, $3, $4)
            RETURNING id, business_id, url, secret, is_active, created_at, updated_at
            "#,
            id,
            business_id,
            url,
            secret,
        )
        .fetch_one(pool)
        .await
    }

    pub async fn list_by_business(
        &self,
        pool: &PgPool,
        business_id: Uuid,
    ) -> Result<Vec<WebhookEndpoint>, sqlx::Error> {
        sqlx::query_as!(
            WebhookEndpoint,
            r#"
            SELECT id, business_id, url, secret, is_active, created_at, updated_at
            FROM webhook_endpoints
            WHERE business_id = $1
            ORDER BY created_at DESC
            "#,
            business_id,
        )
        .fetch_all(pool)
        .await
    }

    pub async fn find_by_id(
        &self,
        pool: &PgPool,
        id: Uuid,
        business_id: Uuid,
    ) -> Result<Option<WebhookEndpoint>, sqlx::Error> {
        sqlx::query_as!(
            WebhookEndpoint,
            r#"
            SELECT id, business_id, url, secret, is_active, created_at, updated_at
            FROM webhook_endpoints
            WHERE id = $1 AND business_id = $2
            LIMIT 1
            "#,
            id,
            business_id,
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn deactivate(
        &self,
        pool: &PgPool,
        id: Uuid,
        business_id: Uuid,
    ) -> Result<Option<WebhookEndpoint>, sqlx::Error> {
        sqlx::query_as!(
            WebhookEndpoint,
            r#"
            UPDATE webhook_endpoints
            SET is_active = false, updated_at = NOW()
            WHERE id = $1 AND business_id = $2
            RETURNING id, business_id, url, secret, is_active, created_at, updated_at
            "#,
            id,
            business_id,
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn list_active_by_business(
        &self,
        pool: &PgPool,
        business_id: Uuid,
    ) -> Result<Vec<WebhookEndpoint>, sqlx::Error> {
        sqlx::query_as!(
            WebhookEndpoint,
            r#"
            SELECT id, business_id, url, secret, is_active, created_at, updated_at
            FROM webhook_endpoints
            WHERE business_id = $1 AND is_active = true
            ORDER BY created_at ASC
            "#,
            business_id,
        )
        .fetch_all(pool)
        .await
    }

    pub async fn create_delivery(
        &self,
        pool: &PgPool,
        id: Uuid,
        endpoint_id: Uuid,
        invoice_id: Option<Uuid>,
        event_type: &str,
        payload: &Value,
    ) -> Result<WebhookDelivery, sqlx::Error> {
        sqlx::query_as!(
            WebhookDelivery,
            r#"
            INSERT INTO webhook_deliveries (
                id, endpoint_id, invoice_id, event_type, payload,
                status, attempt_count, next_retry_at
            )
            VALUES ($1, $2, $3, $4, $5, 'pending', 0, NOW())
            RETURNING
                id, endpoint_id, invoice_id, event_type, payload,
                status, attempt_count, next_retry_at, last_error,
                created_at, updated_at
            "#,
            id,
            endpoint_id,
            invoice_id,
            event_type,
            payload,
        )
        .fetch_one(pool)
        .await
    }

    pub async fn find_due_deliveries(
        &self,
        pool: &PgPool,
        limit: i64,
    ) -> Result<Vec<WebhookDelivery>, sqlx::Error> {
        sqlx::query_as!(
            WebhookDelivery,
            r#"
            SELECT
                id, endpoint_id, invoice_id, event_type, payload,
                status, attempt_count, next_retry_at, last_error,
                created_at, updated_at
            FROM webhook_deliveries
            WHERE status = 'pending'
              AND next_retry_at <= NOW()
            ORDER BY next_retry_at ASC
            LIMIT $1
            "#,
            limit,
        )
        .fetch_all(pool)
        .await
    }

    pub async fn mark_delivered(&self, pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE webhook_deliveries
            SET status = 'delivered', attempt_count = attempt_count + 1, updated_at = NOW()
            WHERE id = $1
            "#,
            id,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    // exponential backoff
    pub async fn mark_failed_or_retry(
        &self,
        pool: &PgPool,
        id: Uuid,
        attempt_count: i32,
        error: &str,
    ) -> Result<(), sqlx::Error> {
        let new_attempt = attempt_count + 1;

        let backoff_minutes: Option<i64> = match new_attempt {
            1 => Some(1),
            2 => Some(5),
            3 => Some(30),
            4 => Some(120),
            5 => Some(480),
            _ => None,
        };

        if let Some(delay_min) = backoff_minutes {
            let next_retry = Utc::now() + Duration::minutes(delay_min);
            sqlx::query!(
                r#"
                UPDATE webhook_deliveries
                SET
                    attempt_count = $2,
                    last_error    = $3,
                    next_retry_at = $4,
                    status        = 'pending',
                    updated_at    = NOW()
                WHERE id = $1
                "#,
                id,
                new_attempt,
                error,
                next_retry,
            )
            .execute(pool)
            .await?;
        } else {
            sqlx::query!(
                r#"
                UPDATE webhook_deliveries
                SET
                    attempt_count = $2,
                    last_error    = $3,
                    status        = 'failed',
                    updated_at    = NOW()
                WHERE id = $1
                "#,
                id,
                new_attempt,
                error,
            )
            .execute(pool)
            .await?;
        }

        Ok(())
    }
}
