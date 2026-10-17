use crate::application::service::BillingApplication;
use crate::domain::entities::{
    BillingTransaction as DomainBillingTransaction, Wallet as DomainWallet,
    WalletLedgerEntry as DomainWalletLedgerEntry,
};
use crate::domain::errors::BillingError;
use chrono::{DateTime, Utc};
use proto::billing_reservation_v1::billing_reservation_service_server::BillingReservationService;
use proto::billing_reservation_v1::billing_wallet_service_server::BillingWalletService;
use proto::billing_reservation_v1::{
    AdjustWalletRequest, AdjustWalletResponse, BillingTransaction, CloseBillingPeriodRequest,
    CloseBillingPeriodResponse, ConfirmDebitRequest, ConfirmDebitResponse, GetWalletRequest,
    GetWalletResponse, ListLedgerRequest, ListLedgerResponse, ListTransactionsRequest,
    ListTransactionsResponse, ReconcileWalletRequest, ReconcileWalletResponse,
    ReleaseReservationRequest, ReleaseReservationResponse, ReserveRequest, ReserveResponse, Wallet,
    WalletLedgerEntry,
};
use rust_decimal::Decimal;
use std::sync::Arc;
use tonic::{Request, Response, Status};
use uuid::Uuid;

#[derive(Clone)]
pub struct BillingGrpcService {
    pub app: Arc<BillingApplication>,
}

fn uuid(raw: &str, field: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(raw).map_err(|e| Status::invalid_argument(format!("invalid {field}: {e}")))
}

fn decimal(raw: &str, field: &str) -> Result<Decimal, Status> {
    raw.parse::<Decimal>()
        .map_err(|e| Status::invalid_argument(format!("invalid {field}: {e}")))
}

fn u64_value(raw: &str, field: &str) -> Result<u64, Status> {
    raw.parse::<u64>()
        .map_err(|e| Status::invalid_argument(format!("invalid {field}: {e}")))
}

fn timestamp(raw: &str, field: &str) -> Result<DateTime<Utc>, Status> {
    DateTime::parse_from_rfc3339(raw)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|e| Status::invalid_argument(format!("invalid {field}: {e}")))
}

fn map_error(error: BillingError) -> Status {
    match error {
        BillingError::WalletNotFound | BillingError::ReservationNotFound => {
            Status::not_found(error.to_string())
        }
        BillingError::InsufficientBalance => Status::failed_precondition(error.to_string()),
        BillingError::ReservationNotPending => Status::failed_precondition(error.to_string()),
        BillingError::DuplicateLedgerEntry => Status::already_exists(error.to_string()),
        BillingError::WalletLedgerIntegrityError(message) => Status::internal(message),
        BillingError::Validation(message) => Status::invalid_argument(message),
        BillingError::DependencyUnavailable(message) => Status::unavailable(message),
    }
}

fn wallet(value: DomainWallet) -> Wallet {
    Wallet {
        user_id: value.user_id.to_string(),
        balance: value.balance.to_string(),
        reserved_balance: value.reserved_balance.to_string(),
        available_balance: value.available_balance().to_string(),
        currency: value.currency,
        low_balance_threshold: value.low_balance_threshold.to_string(),
        updated_at: value.updated_at.to_rfc3339(),
    }
}

fn ledger(value: DomainWalletLedgerEntry) -> WalletLedgerEntry {
    WalletLedgerEntry {
        id: value.id.to_string(),
        user_id: value.user_id.to_string(),
        entry_type: value.entry_type,
        amount: value.amount.to_string(),
        balance_after: value.balance_after.to_string(),
        reference_type: value.reference_type,
        reference_id: value.reference_id.map(|id| id.to_string()),
        idempotency_key: value.idempotency_key,
        created_at: value.created_at.to_rfc3339(),
    }
}

fn transaction(value: DomainBillingTransaction) -> BillingTransaction {
    BillingTransaction {
        id: value.id.to_string(),
        user_id: value.user_id.to_string(),
        r#type: value.transaction_type,
        amount: value.amount.to_string(),
        currency: value.currency,
        provider: value.provider,
        provider_reference: value.provider_reference,
        status: value.status,
        created_at: value.created_at.to_rfc3339(),
    }
}

#[tonic::async_trait]
impl BillingReservationService for BillingGrpcService {
    async fn reserve_balance(
        &self,
        request: Request<ReserveRequest>,
    ) -> Result<Response<ReserveResponse>, Status> {
        let r = request.into_inner();
        let user_id = uuid(&r.user_id, "user_id")?;
        let amount = decimal(&r.estimated_amount, "estimated_amount")?;
        let Some(reservation) = self
            .app
            .reserve_balance(user_id, amount, r.idempotency_key)
            .await
            .map_err(map_error)?
        else {
            let wallet = self.app.get_wallet(user_id).await.map_err(map_error)?;
            return Ok(Response::new(ReserveResponse {
                approved: false,
                reservation_id: String::new(),
                available_balance: wallet
                    .map(|value| value.available_balance().to_string())
                    .unwrap_or_else(|| "0".into()),
            }));
        };
        let wallet = self
            .app
            .get_wallet(user_id)
            .await
            .map_err(map_error)?
            .ok_or_else(|| Status::not_found("wallet not found"))?;
        Ok(Response::new(ReserveResponse {
            approved: true,
            reservation_id: reservation.id.to_string(),
            available_balance: wallet.available_balance().to_string(),
        }))
    }

    async fn release_reservation(
        &self,
        request: Request<ReleaseReservationRequest>,
    ) -> Result<Response<ReleaseReservationResponse>, Status> {
        let r = request.into_inner();
        let released = self
            .app
            .release_reservation(
                uuid(&r.user_id, "user_id")?,
                uuid(&r.reservation_id, "reservation_id")?,
            )
            .await
            .map_err(map_error)?;
        Ok(Response::new(ReleaseReservationResponse { released }))
    }

    async fn confirm_debit(
        &self,
        request: Request<ConfirmDebitRequest>,
    ) -> Result<Response<ConfirmDebitResponse>, Status> {
        let r = request.into_inner();
        let (wallet, charged, aws_cost) = self
            .app
            .confirm_debit(
                uuid(&r.user_id, "user_id")?,
                uuid(&r.reservation_id, "reservation_id")?,
                uuid(&r.request_id, "request_id")?,
                uuid(&r.model_catalog_id, "model_catalog_id")?,
                u64_value(&r.actual_input_tokens, "actual_input_tokens")?,
                u64_value(&r.actual_output_tokens, "actual_output_tokens")?,
            )
            .await
            .map_err(map_error)?;
        Ok(Response::new(ConfirmDebitResponse {
            success: true,
            new_balance: wallet.balance.to_string(),
            charged_amount: charged.to_string(),
            aws_cost: aws_cost.to_string(),
        }))
    }
}

#[tonic::async_trait]
impl BillingWalletService for BillingGrpcService {
    async fn get_wallet(
        &self,
        request: Request<GetWalletRequest>,
    ) -> Result<Response<GetWalletResponse>, Status> {
        let user_id = uuid(&request.into_inner().user_id, "user_id")?;
        let value = self
            .app
            .get_or_create_wallet(user_id)
            .await
            .map_err(map_error)?;
        Ok(Response::new(GetWalletResponse {
            wallet: Some(wallet(value)),
        }))
    }

    async fn list_ledger(
        &self,
        request: Request<ListLedgerRequest>,
    ) -> Result<Response<ListLedgerResponse>, Status> {
        let r = request.into_inner();
        let values = self
            .app
            .list_ledger(uuid(&r.user_id, "user_id")?, i64::from(r.limit))
            .await
            .map_err(map_error)?
            .into_iter()
            .map(ledger)
            .collect();
        Ok(Response::new(ListLedgerResponse { entries: values }))
    }

    async fn list_transactions(
        &self,
        request: Request<ListTransactionsRequest>,
    ) -> Result<Response<ListTransactionsResponse>, Status> {
        let r = request.into_inner();
        let values = self
            .app
            .list_transactions(uuid(&r.user_id, "user_id")?, i64::from(r.limit))
            .await
            .map_err(map_error)?
            .into_iter()
            .map(transaction)
            .collect();
        Ok(Response::new(ListTransactionsResponse {
            transactions: values,
        }))
    }

    async fn adjust_wallet(
        &self,
        request: Request<AdjustWalletRequest>,
    ) -> Result<Response<AdjustWalletResponse>, Status> {
        let r = request.into_inner();
        let value = self
            .app
            .adjust_wallet(
                uuid(&r.user_id, "user_id")?,
                decimal(&r.amount, "amount")?,
                r.currency,
                r.reason,
                r.idempotency_key,
            )
            .await
            .map_err(map_error)?;
        Ok(Response::new(AdjustWalletResponse {
            wallet: Some(wallet(value)),
        }))
    }

    async fn reconcile_wallet(
        &self,
        request: Request<ReconcileWalletRequest>,
    ) -> Result<Response<ReconcileWalletResponse>, Status> {
        let value = self
            .app
            .reconcile_wallet(uuid(&request.into_inner().user_id, "user_id")?)
            .await
            .map_err(map_error)?;
        Ok(Response::new(ReconcileWalletResponse {
            reconciled: true,
            wallet: Some(wallet(value)),
        }))
    }

    async fn close_billing_period(
        &self,
        request: Request<CloseBillingPeriodRequest>,
    ) -> Result<Response<CloseBillingPeriodResponse>, Status> {
        let r = request.into_inner();
        self.app
            .close_billing_period(crate::application::service::CloseBillingPeriodInput {
                user_id: uuid(&r.user_id, "user_id")?,
                subscription_id: uuid(&r.subscription_id, "subscription_id")?,
                period_start: timestamp(&r.period_start, "period_start")?,
                period_end: timestamp(&r.period_end, "period_end")?,
                total_usage_cost: decimal(&r.total_usage_cost, "total_usage_cost")?,
                invoice_id: uuid(&r.invoice_id, "invoice_id")?,
                currency: self.app.config.currency.clone(),
            })
            .await
            .map_err(map_error)?;
        Ok(Response::new(CloseBillingPeriodResponse {
            published: true,
        }))
    }
}
