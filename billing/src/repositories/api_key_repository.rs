use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::domains::models::ApiKey;

pub struct ApiKeyRepository;

impl ApiKeyRepository {
    pub async fn create_api_key(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        api_key_id: Uuid,
        business_id: Uuid,
        key_prefix: &str,
        key_hash: &str,
    ) -> Result<ApiKey, sqlx::Error> {
        let api_key = sqlx::query_as!(
            ApiKey,
            "
            INSERT INTO api_keys (
                id,
                business_id,
                key_prefix,
                key_hash
            )
            VALUES ($1, $2, $3, $4)
            RETURNING
                id,
                business_id,
                key_prefix,
                key_hash,
                is_active,
                created_at,
                revoked_at
            ",
            api_key_id,
            business_id,
            key_prefix,
            key_hash
        )
        .fetch_one(&mut **tx)
        .await?;

        Ok(api_key)
    }

    pub async fn find_by_prefix(
        &self,
        pool: &PgPool,
        key_prefix: &str,
    ) -> Result<Option<ApiKey>, sqlx::Error> {
        let api_key = sqlx::query_as!(
            ApiKey,
            "
            SELECT
                id,
                business_id,
                key_prefix,
                key_hash,
                is_active,
                created_at,
                revoked_at
            FROM api_keys
            WHERE key_prefix = $1 AND is_active = true AND revoked_at IS NULL
            LIMIT 1
            ",
            key_prefix
        )
        .fetch_optional(pool)
        .await?;

        Ok(api_key)
    }

    pub async fn list_by_business(
        &self,
        pool: &PgPool,
        business_id: Uuid,
    ) -> Result<Vec<ApiKey>, sqlx::Error> {
        let api_keys = sqlx::query_as!(
            ApiKey,
            "
            SELECT
                id,
                business_id,
                key_prefix,
                key_hash,
                is_active,
                created_at,
                revoked_at
            FROM api_keys
            WHERE business_id = $1
            ORDER BY created_at DESC
            ",
            business_id
        )
        .fetch_all(pool)
        .await?;

        Ok(api_keys)
    }

    pub async fn find_by_id(
        &self,
        pool: &PgPool,
        api_key_id: Uuid,
        business_id: Uuid,
    ) -> Result<Option<ApiKey>, sqlx::Error> {
        let api_key = sqlx::query_as!(
            ApiKey,
            "
            SELECT
                id,
                business_id,
                key_prefix,
                key_hash,
                is_active,
                created_at,
                revoked_at
            FROM api_keys
            WHERE id = $1 AND business_id = $2
            LIMIT 1
            ",
            api_key_id,
            business_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(api_key)
    }
}
