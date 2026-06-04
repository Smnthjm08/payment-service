use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::domains::models::PaymentAttempt;

pub struct PaymentAttemptRepository;

impl PaymentAttemptRepository {
    /// Insert a new payment attempt in `Pending` status within an existing transaction.
    pub async fn create_attempt(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
        invoice_id: Uuid,
        amount_cents: i64,
        card_token: &str,
    ) -> Result<PaymentAttempt, sqlx::Error> {
        sqlx::query_as!(
            PaymentAttempt,
            r#"
            INSERT INTO payment_attempts (
                id,
                invoice_id,
                status,
                amount_cents,
                card_token
            )
            VALUES ($1, $2, $3, $4, $5)
            RETURNING
                id,
                invoice_id,
                status AS "status: _",
                amount_cents,
                card_token,
                psp_ref,
                psp_response,
                failure_code,
                created_at,
                updated_at
            "#,
            id,
            invoice_id,
            "Pending",
            amount_cents,
            card_token,
        )
        .fetch_one(&mut **tx)
        .await
    }

    /// Update an existing attempt with the final status returned from the PSP.
    pub async fn update_attempt(
        &self,
        pool: &PgPool,
        id: Uuid,
        status: &str,
        psp_ref: Option<&str>,
        psp_response: Option<Value>,
        failure_code: Option<&str>,
    ) -> Result<PaymentAttempt, sqlx::Error> {
        sqlx::query_as!(
            PaymentAttempt,
            r#"
            UPDATE payment_attempts
            SET
                status       = $2,
                psp_ref      = $3,
                psp_response = $4,
                failure_code = $5,
                updated_at   = NOW()
            WHERE id = $1
            RETURNING
                id,
                invoice_id,
                status AS "status: _",
                amount_cents,
                card_token,
                psp_ref,
                psp_response,
                failure_code,
                created_at,
                updated_at
            "#,
            id,
            status,
            psp_ref,
            psp_response,
            failure_code,
        )
        .fetch_one(pool)
        .await
    }

    /// Fetch all payment attempts for a given invoice.
    pub async fn list_by_invoice(
        &self,
        pool: &PgPool,
        invoice_id: Uuid,
    ) -> Result<Vec<PaymentAttempt>, sqlx::Error> {
        sqlx::query_as!(
            PaymentAttempt,
            r#"
            SELECT
                id,
                invoice_id,
                status AS "status: _",
                amount_cents,
                card_token,
                psp_ref,
                psp_response,
                failure_code,
                created_at,
                updated_at
            FROM payment_attempts
            WHERE invoice_id = $1
            ORDER BY created_at ASC
            "#,
            invoice_id,
        )
        .fetch_all(pool)
        .await
    }
}
