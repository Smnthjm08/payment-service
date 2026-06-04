use chrono::{Duration, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domains::models::IdempotencyKey;

pub struct IdempotencyKeyRepository;

impl IdempotencyKeyRepository {

    pub async fn find_by_key(
        &self,
        pool: &PgPool,
        business_id: Uuid,
        key: &str,
    ) -> Result<Option<IdempotencyKey>, sqlx::Error> {
        sqlx::query_as!(
            IdempotencyKey,
            r#"
            SELECT
                id,
                business_id,
                key,
                request_hash,
                response_body,
                status_code,
                created_at,
                expires_at
            FROM idempotency_keys
            WHERE business_id = $1
              AND key = $2
              AND expires_at > NOW()
            LIMIT 1
            "#,
            business_id,
            key,
        )
        .fetch_optional(pool)
        .await
    }

    pub async fn create_key(
        &self,
        pool: &PgPool,
        business_id: Uuid,
        key: &str,
        request_hash: &str,
    ) -> Result<IdempotencyKey, sqlx::Error> {
        let expires_at = Utc::now() + Duration::hours(24);

        sqlx::query_as!(
            IdempotencyKey,
            r#"
            INSERT INTO idempotency_keys (
                id,
                business_id,
                key,
                request_hash,
                expires_at
            )
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (business_id, key) DO NOTHING
            RETURNING
                id,
                business_id,
                key,
                request_hash,
                response_body,
                status_code,
                created_at,
                expires_at
            "#,
            Uuid::new_v4(),
            business_id,
            key,
            request_hash,
            expires_at,
        )
        .fetch_one(pool)
        .await
    }

    pub async fn complete_key(
        &self,
        pool: &PgPool,
        id: Uuid,
        response_body: Value,
        status_code: i32,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE idempotency_keys
            SET
                response_body = $2,
                status_code   = $3
            WHERE id = $1
            "#,
            id,
            response_body,
            status_code,
        )
        .execute(pool)
        .await?;

        Ok(())
    }
}
