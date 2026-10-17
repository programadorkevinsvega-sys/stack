use crate::domain::entities::{Invoice, InvoiceDocument, InvoiceLineItem, LineItemDraft};
use crate::domain::errors::InvoiceError;
use chrono::{DateTime, Datelike, Utc};
use rust_decimal::Decimal;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Clone)]
pub struct PgInvoiceRepository {
    pub pool: PgPool,
    pub signer: common::AuditSigner,
}

pub struct CreateInvoiceInput<'a> {
    pub invoice_id: Uuid,
    pub user_id: Uuid,
    pub subscription_id: Uuid,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub subtotal: Decimal,
    pub tax: Decimal,
    pub total: Decimal,
    pub currency: &'a str,
    pub due_date: DateTime<Utc>,
    pub lines: Vec<LineItemDraft>,
}

impl PgInvoiceRepository {
    pub async fn create_invoice_from_input(
        &self,
        input: CreateInvoiceInput<'_>,
    ) -> Result<InvoiceDocument, InvoiceError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| InvoiceError::DependencyUnavailable(e.to_string()))?;
        let invoice_key = input.invoice_id.to_string();
        sqlx::query!("SELECT pg_advisory_xact_lock(hashtext($1))", &invoice_key)
            .execute(tx.as_mut())
            .await
            .map_err(|e| InvoiceError::DependencyUnavailable(e.to_string()))?;
        if invoice_exists(&mut tx, input.invoice_id).await? {
            tx.commit()
                .await
                .map_err(|e| InvoiceError::DependencyUnavailable(e.to_string()))?;
            return self
                .find_document_by_id(input.invoice_id)
                .await?
                .ok_or(InvoiceError::InvoiceNotFound);
        }
        let invoice_number = next_invoice_number(&mut tx, input.period_end).await?;
        sqlx::query!("INSERT INTO invoices (id,user_id,subscription_id,invoice_number,period_start,period_end,subtotal,tax,total,currency,status,due_date) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'draft',$11)",
            input.invoice_id,
            input.user_id,
            input.subscription_id,
            &invoice_number,
            input.period_start,
            input.period_end,
            input.subtotal,
            input.tax,
            input.total,
            input.currency,
            input.due_date)
            .execute(tx.as_mut())
            .await
            .map_err(|e| InvoiceError::DependencyUnavailable(e.to_string()))?;
        for line in input.lines {
            sqlx::query!("INSERT INTO invoice_line_items (id,invoice_id,description,quantity,unit_price,amount) VALUES ($1,$2,$3,$4,$5,$6)",
                Uuid::now_v7(),
                input.invoice_id,
                line.description,
                line.quantity,
                line.unit_price,
                line.amount)
                .execute(tx.as_mut())
                .await
                .map_err(|e| InvoiceError::DependencyUnavailable(e.to_string()))?;
        }
        tx.commit()
            .await
            .map_err(|e| InvoiceError::DependencyUnavailable(e.to_string()))?;
        self.find_document_by_id(input.invoice_id)
            .await?
            .ok_or(InvoiceError::InvoiceNotFound)
    }

    pub async fn find_invoice_by_id(
        &self,
        invoice_id: Uuid,
    ) -> Result<Option<Invoice>, InvoiceError> {
        sqlx::query_as("SELECT * FROM invoices WHERE id=$1")
            .bind(invoice_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| InvoiceError::DependencyUnavailable(e.to_string()))
    }

    pub async fn find_document_by_id(
        &self,
        invoice_id: Uuid,
    ) -> Result<Option<InvoiceDocument>, InvoiceError> {
        let Some(invoice) = self.find_invoice_by_id(invoice_id).await? else {
            return Ok(None);
        };
        let line_items = self.find_line_items(invoice.id).await?;
        Ok(Some(InvoiceDocument {
            invoice,
            line_items,
        }))
    }

    pub async fn list_documents_by_user(
        &self,
        user_id: Uuid,
        limit: i64,
    ) -> Result<Vec<InvoiceDocument>, InvoiceError> {
        let invoices: Vec<Invoice> = sqlx::query_as(
            "SELECT * FROM invoices WHERE user_id=$1 ORDER BY created_at DESC LIMIT $2",
        )
        .bind(user_id)
        .bind(limit.clamp(1, 200))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| InvoiceError::DependencyUnavailable(e.to_string()))?;
        let mut documents = Vec::with_capacity(invoices.len());
        for invoice in invoices {
            let line_items = self.find_line_items(invoice.id).await?;
            documents.push(InvoiceDocument {
                invoice,
                line_items,
            });
        }
        Ok(documents)
    }

    pub async fn find_line_items(
        &self,
        invoice_id: Uuid,
    ) -> Result<Vec<InvoiceLineItem>, InvoiceError> {
        sqlx::query_as("SELECT * FROM invoice_line_items WHERE invoice_id=$1 ORDER BY id")
            .bind(invoice_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| InvoiceError::DependencyUnavailable(e.to_string()))
    }

    pub async fn update_pdf_url(
        &self,
        invoice_id: Uuid,
        pdf_url: &str,
    ) -> Result<InvoiceDocument, InvoiceError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| InvoiceError::DependencyUnavailable(e.to_string()))?;
        let updated = sqlx::query!(
            "UPDATE invoices SET pdf_url=$2 WHERE id=$1 AND pdf_url IS NULL",
            invoice_id,
            pdf_url
        )
        .execute(tx.as_mut())
        .await
        .map_err(|e| InvoiceError::DependencyUnavailable(e.to_string()))?;
        if updated.rows_affected() == 0 {
            let current = self
                .find_document_by_id(invoice_id)
                .await?
                .ok_or(InvoiceError::InvoiceNotFound)?;
            // pdf_url already set by another generator — return existing document
            return Ok(current);
        }
        tx.commit()
            .await
            .map_err(|e| InvoiceError::DependencyUnavailable(e.to_string()))?;
        self.find_document_by_id(invoice_id)
            .await?
            .ok_or(InvoiceError::InvoiceNotFound)
    }

    pub async fn transition_status(
        &self,
        invoice_id: Uuid,
        from: &str,
        to: &str,
    ) -> Result<InvoiceDocument, InvoiceError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| InvoiceError::DependencyUnavailable(e.to_string()))?;
        let current = sqlx::query_scalar!(
            "SELECT status FROM invoices WHERE id=$1 FOR UPDATE",
            invoice_id,
        )
        .fetch_optional(tx.as_mut())
        .await
        .map_err(|e| InvoiceError::DependencyUnavailable(e.to_string()))?
        .ok_or(InvoiceError::InvoiceNotFound)?;
        if current == to {
            tx.commit()
                .await
                .map_err(|e| InvoiceError::DependencyUnavailable(e.to_string()))?;
            return self
                .find_document_by_id(invoice_id)
                .await?
                .ok_or(InvoiceError::InvoiceNotFound);
        }
        if current != from {
            let err = match current.as_str() {
                "paid" => InvoiceError::InvoiceAlreadyPaid,
                "void" => InvoiceError::InvoiceVoided,
                _ => InvoiceError::InvoiceNotOpen,
            };
            return Err(err);
        }
        let updated = sqlx::query!(
            "UPDATE invoices SET status=$3 WHERE id=$1 AND status=$2",
            invoice_id,
            from,
            to
        )
        .execute(tx.as_mut())
        .await
        .map_err(|e| InvoiceError::DependencyUnavailable(e.to_string()))?;
        if updated.rows_affected() == 0 {
            return Err(InvoiceError::InvoiceNotOpen);
        }
        tx.commit()
            .await
            .map_err(|e| InvoiceError::DependencyUnavailable(e.to_string()))?;
        self.find_document_by_id(invoice_id)
            .await?
            .ok_or(InvoiceError::InvoiceNotFound)
    }

    pub async fn peek_next_invoice_number(
        &self,
        at: DateTime<Utc>,
    ) -> Result<String, InvoiceError> {
        let period_key = period_key(at);
        let next_value = sqlx::query_scalar!(
            "SELECT COALESCE((SELECT next_value FROM invoice_sequences WHERE period_key=$1), 1)",
            &period_key,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| InvoiceError::DependencyUnavailable(e.to_string()))?;
        Ok(format_invoice_number(&period_key, next_value.unwrap_or(1)))
    }

    pub async fn log_audit(
        &self,
        actor_type: &str,
        actor_id: Option<Uuid>,
        action: &str,
        resource_type: &str,
        resource_id: Option<Uuid>,
        metadata: serde_json::Value,
    ) -> Result<(), InvoiceError> {
        let metadata_str = serde_json::to_string(&metadata)
            .map_err(|e| InvoiceError::DependencyUnavailable(e.to_string()))?;
        let sig = self
            .signer
            .sign(
                actor_type,
                actor_id,
                action,
                resource_type,
                resource_id,
                Some(&metadata_str),
            )
            .map_err(InvoiceError::DependencyUnavailable)?;
        sqlx::query(
            "INSERT INTO audit_logs (id,actor_type,actor_id,action,resource_type,resource_id,metadata,signature,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,now())",
        )
        .bind(Uuid::now_v7())
        .bind(actor_type)
        .bind(actor_id)
        .bind(action)
        .bind(resource_type)
        .bind(resource_id)
        .bind(&metadata)
        .bind(sig)
        .execute(&self.pool)
        .await
        .map_err(|e| InvoiceError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }
}

async fn invoice_exists(
    tx: &mut Transaction<'_, Postgres>,
    invoice_id: Uuid,
) -> Result<bool, InvoiceError> {
    sqlx::query_scalar!(
        "SELECT EXISTS (SELECT 1 FROM invoices WHERE id=$1) AS \"exists!\"",
        invoice_id
    )
    .fetch_one(tx.as_mut())
    .await
    .map_err(|e| InvoiceError::DependencyUnavailable(e.to_string()))
}

async fn next_invoice_number(
    tx: &mut Transaction<'_, Postgres>,
    at: DateTime<Utc>,
) -> Result<String, InvoiceError> {
    let key = period_key(at);
    let value = sqlx::query_scalar!(
        "INSERT INTO invoice_sequences (period_key,next_value)
         VALUES ($1,1)
         ON CONFLICT (period_key)
         DO UPDATE SET next_value=invoice_sequences.next_value + 1, updated_at=now()
         RETURNING invoice_sequences.next_value",
        &key,
    )
    .fetch_one(tx.as_mut())
    .await
    .map_err(|e| InvoiceError::DependencyUnavailable(e.to_string()))?;
    Ok(format_invoice_number(&key, value))
}

fn period_key(at: DateTime<Utc>) -> String {
    format!("{:04}{:02}", at.year(), at.month())
}

fn format_invoice_number(period_key: &str, sequence: i64) -> String {
    format!("INV-{period_key}-{sequence:06}")
}
