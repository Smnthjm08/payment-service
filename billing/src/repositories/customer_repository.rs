use sqlx::PgPool;
use uuid::Uuid;

use crate::domains::models::Customer;

pub struct CustomerRepository;

impl CustomerRepository {
    pub async fn create_customer(
        &self,
        pool: &PgPool,
        customer_id: Uuid,
        business_id: Uuid,
        name: &str,
        email: &str,
        phone: Option<&str>,
    ) -> Result<Customer, sqlx::Error> {
        let customer = sqlx::query_as!(
            Customer,
            "
            INSERT INTO customers (
                id,
                business_id,
                name,
                email,
                phone
            )
            VALUES ($1, $2, $3, $4, $5)
            RETURNING
                id,
                business_id,
                name,
                email,
                phone,
                created_at,
                updated_at
            ",
            customer_id,
            business_id,
            name,
            email,
            phone
        )
        .fetch_one(pool)
        .await?;

        Ok(customer)
    }

    pub async fn list_by_business(
        &self,
        pool: &PgPool,
        business_id: Uuid,
    ) -> Result<Vec<Customer>, sqlx::Error> {
        let customers = sqlx::query_as!(
            Customer,
            "
            SELECT
                id,
                business_id,
                name,
                email,
                phone,
                created_at,
                updated_at
            FROM customers
            WHERE business_id = $1
            ORDER BY created_at DESC
            ",
            business_id
        )
        .fetch_all(pool)
        .await?;
        Ok(customers)
    }

    pub async fn find_by_id(
        &self,
        pool: &PgPool,
        customer_id: Uuid,
        business_id: Uuid,
    ) -> Result<Option<Customer>, sqlx::Error> {
        let customer = sqlx::query_as!(
            Customer,
            "
            SELECT
                id,
                business_id,
                name,
                email,
                phone,
                created_at,
                updated_at
            FROM customers
            WHERE id = $1 AND business_id = $2
            LIMIT 1
            ",
            customer_id,
            business_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(customer)
    }
}
