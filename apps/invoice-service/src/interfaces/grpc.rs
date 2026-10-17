use crate::application::service::InvoiceApplication;
use crate::domain::entities::{
    InvoiceDocument as DomainInvoiceDocument, InvoiceLineItem as DomainInvoiceLineItem,
};
use crate::domain::errors::InvoiceError;
use common::ServiceJwtClaims;
use proto::invoice_v1::invoice_service_server::InvoiceService;
use proto::invoice_v1::{
    ChangeInvoiceStatusRequest, GetInvoicePdfRequest, GetInvoicePdfResponse, GetInvoiceRequest,
    GetInvoiceResponse, GetNextInvoiceNumberRequest, GetNextInvoiceNumberResponse, Invoice,
    InvoiceLineItem, InvoiceResponse, ListInvoicesByUserRequest, ListInvoicesByUserResponse,
};
use std::collections::HashSet;
use std::sync::Arc;
use tonic::{Request, Response, Status};
use uuid::Uuid;

#[derive(Clone)]
pub struct InvoiceGrpcService {
    pub app: Arc<InvoiceApplication>,
    pub admin_callers: HashSet<String>,
    pub user_access_callers: HashSet<String>,
}

fn uuid(raw: &str, field: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(raw).map_err(|e| Status::invalid_argument(format!("invalid {field}: {e}")))
}

fn require_admin<T>(request: &Request<T>, admin_callers: &HashSet<String>) -> Result<(), Status> {
    let claims = request
        .extensions()
        .get::<ServiceJwtClaims>()
        .ok_or_else(|| Status::unauthenticated("missing service claims"))?;
    if admin_callers.contains(&claims.sub) {
        Ok(())
    } else {
        Err(Status::permission_denied(
            "caller is not allowed for invoice admin operations",
        ))
    }
}

fn require_user_access<T>(
    request: &Request<T>,
    admin_callers: &HashSet<String>,
    user_access_callers: &HashSet<String>,
) -> Result<(), Status> {
    let claims = request
        .extensions()
        .get::<ServiceJwtClaims>()
        .ok_or_else(|| Status::unauthenticated("missing service claims"))?;
    if admin_callers.contains(&claims.sub) || user_access_callers.contains(&claims.sub) {
        Ok(())
    } else {
        Err(Status::permission_denied(
            "caller is not allowed to access invoice data",
        ))
    }
}

fn admin_subject<T>(
    request: &Request<T>,
    admin_callers: &HashSet<String>,
) -> Result<String, Status> {
    let claims = request
        .extensions()
        .get::<ServiceJwtClaims>()
        .ok_or_else(|| Status::unauthenticated("missing service claims"))?;
    if admin_callers.contains(&claims.sub) {
        Ok(claims.sub.clone())
    } else {
        Err(Status::permission_denied(
            "caller is not allowed for invoice admin operations",
        ))
    }
}

fn map_error(error: InvoiceError) -> Status {
    match error {
        InvoiceError::InvoiceNotFound => Status::not_found(error.to_string()),
        InvoiceError::InvoiceNotDraft
        | InvoiceError::InvoiceNotOpen
        | InvoiceError::InvoiceAlreadyPaid
        | InvoiceError::InvoiceVoided => Status::failed_precondition(error.to_string()),
        InvoiceError::InvalidPeriod | InvoiceError::Validation(_) => {
            Status::invalid_argument(error.to_string())
        }
        InvoiceError::PdfGenerationFailed(_)
        | InvoiceError::PdfStorageFailed(_)
        | InvoiceError::DependencyUnavailable(_) => {
            Status::unavailable("service temporarily unavailable")
        }
    }
}

fn invoice(document: DomainInvoiceDocument) -> Invoice {
    Invoice {
        id: document.invoice.id.to_string(),
        user_id: document.invoice.user_id.to_string(),
        subscription_id: document.invoice.subscription_id.map(|id| id.to_string()),
        invoice_number: document.invoice.invoice_number,
        period_start: document.invoice.period_start.to_rfc3339(),
        period_end: document.invoice.period_end.to_rfc3339(),
        subtotal: document.invoice.subtotal.to_string(),
        tax: document.invoice.tax.to_string(),
        total: document.invoice.total.to_string(),
        currency: document.invoice.currency,
        status: document.invoice.status,
        due_date: document.invoice.due_date.map(|value| value.to_rfc3339()),
        paid_at: document.invoice.paid_at.map(|value| value.to_rfc3339()),
        pdf_url: document.invoice.pdf_url,
        created_at: document.invoice.created_at.to_rfc3339(),
        line_items: document.line_items.into_iter().map(line_item).collect(),
    }
}

fn line_item(value: DomainInvoiceLineItem) -> InvoiceLineItem {
    InvoiceLineItem {
        id: value.id.to_string(),
        invoice_id: value.invoice_id.to_string(),
        description: value.description,
        quantity: value.quantity.to_string(),
        unit_price: value.unit_price.to_string(),
        amount: value.amount.to_string(),
    }
}

#[tonic::async_trait]
impl InvoiceService for InvoiceGrpcService {
    async fn get_invoice(
        &self,
        request: Request<GetInvoiceRequest>,
    ) -> Result<Response<GetInvoiceResponse>, Status> {
        require_user_access(&request, &self.admin_callers, &self.user_access_callers)?;
        let value = self
            .app
            .get_invoice(uuid(&request.into_inner().invoice_id, "invoice_id")?)
            .await
            .map_err(map_error)?
            .map(invoice);
        Ok(Response::new(GetInvoiceResponse { invoice: value }))
    }

    async fn list_invoices_by_user(
        &self,
        request: Request<ListInvoicesByUserRequest>,
    ) -> Result<Response<ListInvoicesByUserResponse>, Status> {
        require_user_access(&request, &self.admin_callers, &self.user_access_callers)?;
        let r = request.into_inner();
        let invoices = self
            .app
            .list_invoices_by_user(uuid(&r.user_id, "user_id")?, i64::from(r.limit))
            .await
            .map_err(map_error)?
            .into_iter()
            .map(invoice)
            .collect();
        Ok(Response::new(ListInvoicesByUserResponse { invoices }))
    }

    async fn get_invoice_pdf(
        &self,
        request: Request<GetInvoicePdfRequest>,
    ) -> Result<Response<GetInvoicePdfResponse>, Status> {
        require_user_access(&request, &self.admin_callers, &self.user_access_callers)?;
        let (pdf_url, pdf) = self
            .app
            .get_invoice_pdf(uuid(&request.into_inner().invoice_id, "invoice_id")?)
            .await
            .map_err(map_error)?;
        Ok(Response::new(GetInvoicePdfResponse {
            content_type: "application/pdf".into(),
            pdf,
            pdf_url,
        }))
    }

    async fn get_next_invoice_number(
        &self,
        request: Request<GetNextInvoiceNumberRequest>,
    ) -> Result<Response<GetNextInvoiceNumberResponse>, Status> {
        require_admin(&request, &self.admin_callers)?;
        Ok(Response::new(GetNextInvoiceNumberResponse {
            invoice_number: self.app.next_invoice_number().await.map_err(map_error)?,
        }))
    }

    async fn open_invoice(
        &self,
        request: Request<ChangeInvoiceStatusRequest>,
    ) -> Result<Response<InvoiceResponse>, Status> {
        let actor_subject = admin_subject(&request, &self.admin_callers)?;
        let document = self
            .app
            .open_invoice(
                uuid(&request.into_inner().invoice_id, "invoice_id")?,
                actor_subject,
            )
            .await
            .map_err(map_error)?;
        Ok(Response::new(InvoiceResponse {
            invoice: Some(invoice(document)),
        }))
    }

    async fn mark_invoice_paid(
        &self,
        request: Request<ChangeInvoiceStatusRequest>,
    ) -> Result<Response<InvoiceResponse>, Status> {
        let actor_subject = admin_subject(&request, &self.admin_callers)?;
        let document = self
            .app
            .mark_paid(
                uuid(&request.into_inner().invoice_id, "invoice_id")?,
                actor_subject,
            )
            .await
            .map_err(map_error)?;
        Ok(Response::new(InvoiceResponse {
            invoice: Some(invoice(document)),
        }))
    }

    async fn void_invoice(
        &self,
        request: Request<ChangeInvoiceStatusRequest>,
    ) -> Result<Response<InvoiceResponse>, Status> {
        let actor_subject = admin_subject(&request, &self.admin_callers)?;
        let document = self
            .app
            .void_invoice(
                uuid(&request.into_inner().invoice_id, "invoice_id")?,
                actor_subject,
            )
            .await
            .map_err(map_error)?;
        Ok(Response::new(InvoiceResponse {
            invoice: Some(invoice(document)),
        }))
    }

    async fn mark_invoice_uncollectible(
        &self,
        request: Request<ChangeInvoiceStatusRequest>,
    ) -> Result<Response<InvoiceResponse>, Status> {
        let actor_subject = admin_subject(&request, &self.admin_callers)?;
        let document = self
            .app
            .mark_uncollectible(
                uuid(&request.into_inner().invoice_id, "invoice_id")?,
                actor_subject,
            )
            .await
            .map_err(map_error)?;
        Ok(Response::new(InvoiceResponse {
            invoice: Some(invoice(document)),
        }))
    }
}
