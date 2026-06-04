use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;
use chrono::{DateTime, Utc};

use crate::domains::models::{Invoice, InvoiceLineItem};

pub struct InvoiceRepository;

impl InvoiceRepository {
    pub async fn create_invoice(
        &self,
        pool: &PgPool,
        invoice_id: Uuid,
        business_id: Uuid,
        customer_id: Uuid,
        due_date: DateTime<Utc>,
        line_items: Vec<(Uuid, String, i32, i64)>, // (id, description, quantity, unit_amount_cents)
    ) -> Result<(Invoice, Vec<InvoiceLineItem>), sqlx::Error> {
        let total_amount_cents: i64 = line_items
            .iter()
            .map(|(_, _, qty, unit)| (*qty as i64) * (*unit))
            .sum();

            let mut tx: Transaction<'_, Postgres> = pool.begin().await?;

        let invoice = sqlx::query_as!(
            Invoice,
            r#"
            INSERT INTO invoices (
                id,
                business_id,
                customer_id,
                state,
                total_amount_cents,
                due_date
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING
                id,
                business_id,
                customer_id,
                state,
                total_amount_cents,
                due_date,
                created_at,
                updated_at,
                paid_at
            "#,
            invoice_id,
            business_id,
            customer_id,
            "Open",
            total_amount_cents,
            due_date
        )
        .fetch_one(&mut *tx)
        .await?;

        let mut inserted_items = Vec::with_capacity(line_items.len());

        for (item_id, description, quantity, unit_amount_cents) in line_items.into_iter() {
            let item = sqlx::query_as!(
                InvoiceLineItem,
                r#"
                INSERT INTO invoice_line_items (
                    id,
                    invoice_id,
                    description,
                    quantity,
                    unit_amount_cents
                )
                VALUES ($1, $2, $3, $4, $5)
                RETURNING
                    id,
                    invoice_id,
                    description,
                    quantity,
                    unit_amount_cents,
                    created_at,
                    updated_at
                "#,
                item_id,
                invoice_id,
                description,
                quantity,
                unit_amount_cents
            )
            .fetch_one(&mut *tx)
            .await?;

            inserted_items.push(item);
        }

        // commit the transaction
        tx.commit().await?;
        Ok((invoice, inserted_items))
    }

    pub async fn find_by_id(
        &self,
        pool: &PgPool,
        invoice_id: Uuid,
        business_id: Uuid,
    ) -> Result<Option<(Invoice, Vec<InvoiceLineItem>)>, sqlx::Error> {
        let invoice = sqlx::query_as!(
            Invoice,
            r#"
            SELECT
                id,
                business_id,
                customer_id,
                state,
                total_amount_cents,
                due_date,
                created_at,
                updated_at,
                paid_at
            FROM invoices
            WHERE id = $1 AND business_id = $2
            LIMIT 1
            "#,
            invoice_id,
            business_id
        )
        .fetch_optional(pool)
        .await?;

        if let Some(inv) = invoice {
            let items = sqlx::query_as!(
                InvoiceLineItem,
                r#"
                SELECT
                    id,
                    invoice_id,
                    description,
                    quantity,
                    unit_amount_cents,
                    created_at,
                    updated_at
                FROM invoice_line_items
                WHERE invoice_id = $1
                ORDER BY created_at ASC
                "#,
                inv.id
            )
            .fetch_all(pool)
            .await?;

            Ok(Some((inv, items)))
        } else {
            Ok(None)
        }
    }

    pub async fn list_by_business(
        &self,
        pool: &PgPool,
        business_id: Uuid,
        state_filter: Option<&str>,
    ) -> Result<Vec<(Invoice, Vec<InvoiceLineItem>)>, sqlx::Error> {
        let invoices = if let Some(state_val) = state_filter {
            sqlx::query_as!(
                Invoice,
                r#"
                SELECT
                    id,
                    business_id,
                    customer_id,
                    state,
                    total_amount_cents,
                    due_date,
                    created_at,
                    updated_at,
                    paid_at
                FROM invoices
                WHERE business_id = $1 AND state = $2
                ORDER BY created_at DESC
                "#,
                business_id,
                state_val
            )
            .fetch_all(pool)
            .await?
        } else {
            sqlx::query_as!(
                Invoice,
                r#"
                SELECT
                    id,
                    business_id,
                    customer_id,
                    state,
                    total_amount_cents,
                    due_date,
                    created_at,
                    updated_at,
                    paid_at
                FROM invoices
                WHERE business_id = $1
                ORDER BY created_at DESC
                "#,
                business_id
            )
            .fetch_all(pool)
            .await?
        };

        let mut results = Vec::with_capacity(invoices.len());

        for inv in invoices.into_iter() {
            let items = sqlx::query_as!(
                InvoiceLineItem,
                r#"
                SELECT
                    id,
                    invoice_id,
                    description,
                    quantity,
                    unit_amount_cents,
                    created_at,
                    updated_at
                FROM invoice_line_items
                WHERE invoice_id = $1
                ORDER BY created_at ASC
                "#,
                inv.id
            )
            .fetch_all(pool)
            .await?;

            results.push((inv, items));
        }

        Ok(results)
    }
}
