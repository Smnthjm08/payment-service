use sqlx::PgPool;
use uuid::Uuid;

use crate::domains::models::Business;

pub struct BusinessRepository;

impl BusinessRepository {
    pub async fn create_business(
        &self,
        pool: &PgPool,
        business_id: Uuid,
        email: &str,
        name: &str,
    ) -> Result<Business, sqlx::Error> {
        let business = sqlx::query_as!(
            Business,
            "
            INSERT INTO businesses (
                id,
                email,
                name
            )
            VALUES ($1, $2, $3)
            RETURNING
                id,
                email,
                name,
                is_active,
                created_at,
                updated_at
            ",
            business_id,
            email,
            name
        )
        .fetch_one(pool)
        .await?;

        Ok(business)
    }
}
