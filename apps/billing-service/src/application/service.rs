use crate::domain::entities::{BillingTransaction, Reservation, Wallet, WalletLedgerEntry};
use crate::domain::errors::BillingError;
use crate::infrastructure::catalog::PricingResolver;
use crate::infrastructure::messaging::BillingEventPublisher;
use crate::infrastructure::persistence::{CreditWalletInput, PgBillingRepository};
use chrono::{DateTime, Utc};
use events::{BillingPeriodClosed, PaymentSucceeded, UsageRecorded};
use rust_decimal::Decimal;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct BillingConfig {
    pub currency: String,
    pub low_balance_threshold: Decimal,
    pub reservation_ttl_minutes: i64,
    pub input_token_price_per_1k: Decimal,
    pub output_token_price_per_1k: Decimal,
}

#[derive(Clone)]
pub struct BillingApplication {
    pub repo: Arc<PgBillingRepository>,
    pub events: Arc<BillingEventPublisher>,
    pub pricing: Arc<dyn PricingResolver>,
    pub config: BillingConfig,
}

impl BillingApplication {
    pub async fn get_wallet(&self, user_id: Uuid) -> Result<Option<Wallet>, BillingError> {
        self.repo.get_wallet(user_id).await
    }

    pub async fn get_or_create_wallet(&self, user_id: Uuid) -> Result<Wallet, BillingError> {
        self.repo
            .get_or_create_wallet(
                user_id,
                &self.config.currency,
                self.config.low_balance_threshold,
            )
            .await
    }

    pub async fn list_ledger(
        &self,
        user_id: Uuid,
        limit: i64,
    ) -> Result<Vec<WalletLedgerEntry>, BillingError> {
        self.repo.list_ledger(user_id, limit).await
    }

    pub async fn list_transactions(
        &self,
        user_id: Uuid,
        limit: i64,
    ) -> Result<Vec<BillingTransaction>, BillingError> {
        self.repo.list_transactions(user_id, limit).await
    }

    pub async fn reserve_balance(
        &self,
        user_id: Uuid,
        amount: Decimal,
        idempotency_key: String,
    ) -> Result<Option<Reservation>, BillingError> {
        if amount <= Decimal::ZERO {
            return Err(BillingError::Validation("amount must be positive".into()));
        }
        self.repo
            .reserve_balance(
                user_id,
                amount,
                &self.config.currency,
                idempotency_key,
                self.config.low_balance_threshold,
                self.config.reservation_ttl_minutes,
            )
            .await
    }

    pub async fn release_reservation(
        &self,
        user_id: Uuid,
        reservation_id: Uuid,
    ) -> Result<bool, BillingError> {
        self.repo.release_reservation(user_id, reservation_id).await
    }

    pub async fn confirm_debit(
        &self,
        user_id: Uuid,
        reservation_id: Uuid,
        request_id: Uuid,
        model_catalog_id: Uuid,
        input_tokens: u64,
        output_tokens: u64,
    ) -> Result<(Wallet, Decimal, Decimal), BillingError> {
        let pricing = self.pricing.get_current_pricing(model_catalog_id).await?;
        let charged = calculate_charge(
            input_tokens,
            output_tokens,
            pricing.price_per_1k_input,
            pricing.price_per_1k_output,
        );
        let aws_cost = calculate_charge(
            input_tokens,
            output_tokens,
            pricing.aws_cost_per_1k_input,
            pricing.aws_cost_per_1k_output,
        );
        let wallet = self
            .repo
            .confirm_debit(
                user_id,
                reservation_id,
                request_id,
                model_catalog_id,
                charged,
                aws_cost,
            )
            .await?;
        Ok((wallet, charged, aws_cost))
    }

    pub async fn process_payment_succeeded(
        &self,
        event: PaymentSucceeded,
    ) -> Result<Wallet, BillingError> {
        let wallet = self
            .repo
            .credit_wallet(CreditWalletInput {
                user_id: event.user_id,
                amount: event.amount,
                currency: &event.currency,
                provider: &event.provider,
                provider_reference: &event.provider_reference,
                payment_transaction_id: event.transaction_id,
                low_balance_threshold: self.config.low_balance_threshold,
            })
            .await?;
        Ok(wallet)
    }

    pub async fn process_usage_recorded(&self, event: UsageRecorded) -> Result<(), BillingError> {
        if event.status_code >= 400 {
            return Ok(());
        }
        let Some(reservation) = self
            .repo
            .find_pending_reservation_for_request(event.user_id, event.request_id)
            .await?
        else {
            tracing::info!(
                user_id = %event.user_id,
                request_id = %event.request_id,
                "usage event already applied or has no pending reservation"
            );
            return Ok(());
        };
        self.confirm_debit(
            event.user_id,
            reservation.id,
            event.request_id,
            event.model_catalog_id,
            u64::from(event.input_tokens),
            u64::from(event.output_tokens),
        )
        .await?;
        Ok(())
    }

    pub async fn adjust_wallet(
        &self,
        user_id: Uuid,
        amount: Decimal,
        currency: String,
        reason: String,
        idempotency_key: String,
    ) -> Result<Wallet, BillingError> {
        if reason.trim().is_empty() || idempotency_key.trim().is_empty() {
            return Err(BillingError::Validation(
                "reason and idempotency_key are required".into(),
            ));
        }
        let wallet = self
            .repo
            .adjust_wallet(
                user_id,
                amount,
                &currency,
                &reason,
                &idempotency_key,
                self.config.low_balance_threshold,
            )
            .await?;
        self.repo
            .log_audit(
                "admin",
                None,
                "billing.wallet.adjusted",
                "wallet",
                Some(user_id),
                serde_json::json!({
                    "amount": amount,
                    "currency": currency,
                    "reason": reason,
                    "idempotency_key": idempotency_key
                }),
            )
            .await?;
        Ok(wallet)
    }

    pub async fn reconcile_wallet(&self, user_id: Uuid) -> Result<Wallet, BillingError> {
        self.repo.reconcile_wallet(user_id).await
    }

    pub async fn expire_overdue_reservations(&self) -> Result<u64, BillingError> {
        self.repo.expire_overdue_reservations(500).await
    }

    pub async fn close_billing_period(
        &self,
        input: CloseBillingPeriodInput,
    ) -> Result<(), BillingError> {
        self.events
            .publish_billing_period_closed(&BillingPeriodClosed {
                user_id: input.user_id,
                subscription_id: input.subscription_id,
                period_start: input.period_start,
                period_end: input.period_end,
                total_usage_cost: input.total_usage_cost,
                currency: input.currency,
                invoice_id: input.invoice_id,
            })
            .await
    }
}

pub struct CloseBillingPeriodInput {
    pub user_id: Uuid,
    pub subscription_id: Uuid,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub total_usage_cost: Decimal,
    pub invoice_id: Uuid,
    pub currency: String,
}

fn calculate_charge(
    input_tokens: u64,
    output_tokens: u64,
    input_price_per_1k: Decimal,
    output_price_per_1k: Decimal,
) -> Decimal {
    let input = Decimal::from(input_tokens) * input_price_per_1k / Decimal::from(1000u32);
    let output = Decimal::from(output_tokens) * output_price_per_1k / Decimal::from(1000u32);
    (input + output).round_dp(4)
}
