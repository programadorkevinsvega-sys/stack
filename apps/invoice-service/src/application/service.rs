use crate::domain::entities::{InvoiceDocument, LineItemDraft};
use crate::domain::errors::InvoiceError;
use crate::infrastructure::pdf::{PdfGenerator, PdfStorage};
use crate::infrastructure::persistence::{CreateInvoiceInput, PgInvoiceRepository};
use async_trait::async_trait;
use chrono::{Duration, Utc};
use events::BillingPeriodClosed;
use rust_decimal::Decimal;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct InvoiceConfig {
    pub currency: String,
    pub tax_rate: Decimal,
    pub due_days: i64,
}

#[derive(Clone)]
pub struct InvoiceApplication {
    pub repo: Arc<PgInvoiceRepository>,
    pub pdf_generator: Arc<dyn PdfGenerator>,
    pub pdf_storage: Arc<dyn PdfStorage>,
    pub config: InvoiceConfig,
}

#[async_trait]
pub trait BillingPeriodProcessor: Send + Sync {
    async fn process_billing_period_closed(
        &self,
        event: BillingPeriodClosed,
    ) -> Result<InvoiceDocument, InvoiceError>;
}

#[async_trait]
impl BillingPeriodProcessor for InvoiceApplication {
    async fn process_billing_period_closed(
        &self,
        event: BillingPeriodClosed,
    ) -> Result<InvoiceDocument, InvoiceError> {
        self.generate_from_billing_period(event).await
    }
}

impl InvoiceApplication {
    pub async fn generate_from_billing_period(
        &self,
        event: BillingPeriodClosed,
    ) -> Result<InvoiceDocument, InvoiceError> {
        if event.period_start >= event.period_end {
            return Err(InvoiceError::InvalidPeriod);
        }
        if let Some(existing) = self.repo.find_document_by_id(event.invoice_id).await? {
            if existing.invoice.period_start == event.period_start
                && existing.invoice.period_end == event.period_end
            {
                return Ok(existing);
            }
            tracing::warn!(
                invoice_id = %event.invoice_id,
                existing_period_start = %existing.invoice.period_start,
                existing_period_end = %existing.invoice.period_end,
                event_period_start = %event.period_start,
                event_period_end = %event.period_end,
                "invoice_id collision with different billing period — returning stale, investigate"
            );
            return Ok(existing);
        }

        let subtotal = event.total_usage_cost.round_dp(4);
        let tax = (subtotal * self.config.tax_rate).round_dp(4);
        let total = (subtotal + tax).round_dp(4);
        let line_items = vec![LineItemDraft {
            description: "Uso del período de facturación".into(),
            quantity: rust_decimal::Decimal::ONE,
            unit_price: subtotal.round_dp(6),
            amount: subtotal,
        }];
        let due_date = event.period_end + Duration::days(self.config.due_days);
        let invoice = self
            .repo
            .create_invoice_from_input(CreateInvoiceInput {
                invoice_id: event.invoice_id,
                user_id: event.user_id,
                subscription_id: event.subscription_id,
                period_start: event.period_start,
                period_end: event.period_end,
                subtotal,
                tax,
                total,
                currency: &self.config.currency,
                due_date,
                lines: line_items,
            })
            .await?;
        let pdf = self
            .pdf_generator
            .generate_invoice_pdf(&invoice)
            .await
            .map_err(|e| InvoiceError::PdfGenerationFailed(e.to_string()))?;
        let pdf_url = self
            .pdf_storage
            .store_invoice_pdf(&invoice.invoice, pdf)
            .await?;
        let document = self
            .repo
            .update_pdf_url(invoice.invoice.id, &pdf_url)
            .await?;
        self.repo
            .transition_status(invoice.invoice.id, "draft", "open")
            .await?;
        Ok(document)
    }

    pub async fn get_invoice(
        &self,
        invoice_id: Uuid,
    ) -> Result<Option<InvoiceDocument>, InvoiceError> {
        self.repo.find_document_by_id(invoice_id).await
    }

    pub async fn list_invoices_by_user(
        &self,
        user_id: Uuid,
        limit: i64,
    ) -> Result<Vec<InvoiceDocument>, InvoiceError> {
        self.repo.list_documents_by_user(user_id, limit).await
    }

    pub async fn get_invoice_pdf(
        &self,
        invoice_id: Uuid,
    ) -> Result<(String, Vec<u8>), InvoiceError> {
        let invoice = self
            .repo
            .find_invoice_by_id(invoice_id)
            .await?
            .ok_or(InvoiceError::InvoiceNotFound)?;
        let pdf_url = invoice
            .pdf_url
            .as_deref()
            .ok_or_else(|| InvoiceError::Validation("invoice has no pdf_url".into()))?;
        let bytes = self.pdf_storage.retrieve_invoice_pdf(pdf_url).await?;
        Ok((pdf_url.to_owned(), bytes))
    }

    pub async fn next_invoice_number(&self) -> Result<String, InvoiceError> {
        self.repo.peek_next_invoice_number(Utc::now()).await
    }

    pub async fn open_invoice(
        &self,
        invoice_id: Uuid,
        actor_subject: String,
    ) -> Result<InvoiceDocument, InvoiceError> {
        self.transition_with_audit(invoice_id, "draft", "open", "invoice.opened", actor_subject)
            .await
    }

    pub async fn mark_paid(
        &self,
        invoice_id: Uuid,
        actor_subject: String,
    ) -> Result<InvoiceDocument, InvoiceError> {
        let document = self
            .repo
            .transition_status(invoice_id, "open", "paid")
            .await?;
        self.audit_admin_status_change(&document, "invoice.paid", actor_subject)
            .await?;
        Ok(document)
    }

    pub async fn void_invoice(
        &self,
        invoice_id: Uuid,
        actor_subject: String,
    ) -> Result<InvoiceDocument, InvoiceError> {
        self.transition_with_audit(invoice_id, "draft", "void", "invoice.voided", actor_subject)
            .await
    }

    pub async fn mark_uncollectible(
        &self,
        invoice_id: Uuid,
        actor_subject: String,
    ) -> Result<InvoiceDocument, InvoiceError> {
        self.transition_with_audit(
            invoice_id,
            "open",
            "uncollectible",
            "invoice.uncollectible",
            actor_subject,
        )
        .await
    }

    async fn transition_with_audit(
        &self,
        invoice_id: Uuid,
        from: &str,
        to: &str,
        action: &str,
        actor_subject: String,
    ) -> Result<InvoiceDocument, InvoiceError> {
        let document = self.repo.transition_status(invoice_id, from, to).await?;
        self.audit_admin_status_change(&document, action, actor_subject)
            .await?;
        Ok(document)
    }

    async fn audit_admin_status_change(
        &self,
        document: &InvoiceDocument,
        action: &str,
        actor_subject: String,
    ) -> Result<(), InvoiceError> {
        self.repo
            .log_audit(
                "admin",
                None,
                action,
                "invoice",
                Some(document.invoice.id),
                serde_json::json!({
                    "actor_subject": actor_subject,
                    "invoice_number": document.invoice.invoice_number,
                    "user_id": document.invoice.user_id,
                    "status": document.invoice.status,
                    "total": document.invoice.total,
                    "currency": document.invoice.currency
                }),
            )
            .await
    }
}
