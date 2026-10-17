use crate::domain::entities::{BillingTransaction, Reservation, Wallet, WalletLedgerEntry};
use crate::domain::errors::BillingError;
use chrono::{Duration, Utc};
use events::{
    outbox::enqueue_in_transaction, EventEnvelope, WalletDebited, WalletLowBalance,
    EVENT_WALLET_DEBITED, EVENT_WALLET_LOW_BALANCE,
};
use rust_decimal::Decimal;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Clone)]
pub struct PgBillingRepository {
    pub pool: PgPool,
    pub signer: common::AuditSigner,
}

pub struct CreditWalletInput<'a> {
    pub user_id: Uuid,
    pub amount: Decimal,
    pub currency: &'a str,
    pub provider: &'a str,
    pub provider_reference: &'a str,
    pub payment_transaction_id: Uuid,
    pub low_balance_threshold: Decimal,
}

struct LedgerInsert<'a> {
    user_id: Uuid,
    entry_type: &'a str,
    amount: Decimal,
    balance_after: Decimal,
    reference_type: &'a str,
    reference_id: Option<Uuid>,
    idempotency_key: &'a str,
}

struct TransactionInsert<'a> {
    user_id: Uuid,
    transaction_type: &'a str,
    amount: Decimal,
    currency: &'a str,
    provider: Option<&'a str>,
    provider_reference: Option<&'a str>,
    status: &'a str,
}

#[derive(Clone, sqlx::FromRow)]
pub struct WebhookEndpoint {
    pub id: Uuid,
    pub user_id: Uuid,
    pub url: String,
    #[sqlx(rename = "signing_secret_encrypted")]
    pub signing_secret: Vec<u8>,
    pub subscribed_events: Vec<String>,
    pub is_active: bool,
}

impl WebhookEndpoint {
    pub fn validate_url(url: &str) -> Result<(), BillingError> {
        common::ssrf::validate_url(url).map_err(|e| BillingError::Validation(e.to_string()))
    }
}

impl PgBillingRepository {
    pub async fn get_wallet(&self, user_id: Uuid) -> Result<Option<Wallet>, BillingError> {
        sqlx::query_as("SELECT * FROM wallets WHERE user_id=$1")
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))
    }

    pub async fn get_or_create_wallet(
        &self,
        user_id: Uuid,
        currency: &str,
        low_balance_threshold: Decimal,
    ) -> Result<Wallet, BillingError> {
        sqlx::query!("INSERT INTO wallets (user_id,currency,low_balance_threshold) VALUES ($1,$2,$3) ON CONFLICT (user_id) DO NOTHING",
            user_id,
            currency,
            low_balance_threshold)
            .execute(&self.pool)
            .await
            .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        self.get_wallet(user_id)
            .await?
            .ok_or(BillingError::WalletNotFound)
    }

    pub async fn list_ledger(
        &self,
        user_id: Uuid,
        limit: i64,
    ) -> Result<Vec<WalletLedgerEntry>, BillingError> {
        sqlx::query_as(
            "SELECT * FROM wallet_ledger WHERE user_id=$1 ORDER BY created_at DESC LIMIT $2",
        )
        .bind(user_id)
        .bind(limit.clamp(1, 200))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))
    }

    pub async fn list_transactions(
        &self,
        user_id: Uuid,
        limit: i64,
    ) -> Result<Vec<BillingTransaction>, BillingError> {
        sqlx::query_as(
            "SELECT * FROM transactions WHERE user_id=$1 ORDER BY created_at DESC LIMIT $2",
        )
        .bind(user_id)
        .bind(limit.clamp(1, 200))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))
    }

    pub async fn find_reservation_by_idempotency_key(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<Reservation>, BillingError> {
        sqlx::query_as("SELECT * FROM reservations WHERE idempotency_key=$1")
            .bind(idempotency_key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))
    }

    pub async fn find_pending_reservation_for_request(
        &self,
        user_id: Uuid,
        request_id: Uuid,
    ) -> Result<Option<Reservation>, BillingError> {
        sqlx::query_as("SELECT * FROM reservations WHERE user_id=$1 AND status='pending' AND idempotency_key=$2")
            .bind(user_id)
            .bind(request_id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))
    }

    pub async fn reserve_balance(
        &self,
        user_id: Uuid,
        amount: Decimal,
        currency: &str,
        idempotency_key: String,
        low_balance_threshold: Decimal,
        reservation_ttl_minutes: i64,
    ) -> Result<Option<Reservation>, BillingError> {
        if let Some(existing) = self
            .find_reservation_by_idempotency_key(&idempotency_key)
            .await?
        {
            return Ok(Some(existing));
        }
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        ensure_wallet(&mut tx, user_id, currency, low_balance_threshold).await?;
        let wallet = lock_wallet(&mut tx, user_id).await?;
        if wallet.available_balance() < amount {
            tx.rollback()
                .await
                .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
            return Ok(None);
        }
        let reservation_id = Uuid::now_v7();
        let expires_at = Utc::now() + Duration::minutes(reservation_ttl_minutes);
        let reservation: Reservation = sqlx::query_as("INSERT INTO reservations (id,user_id,amount,currency,status,idempotency_key,expires_at) VALUES ($1,$2,$3,$4,'pending',$5,$6) RETURNING *")
            .bind(reservation_id)
            .bind(user_id)
            .bind(amount)
            .bind(currency)
            .bind(&idempotency_key)
            .bind(expires_at)
            .fetch_one(tx.as_mut())
            .await
            .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        let updated = wallet.reserved_balance + amount;
        sqlx::query!(
            "UPDATE wallets SET reserved_balance=$2, updated_at=now() WHERE user_id=$1",
            user_id,
            updated,
        )
        .execute(tx.as_mut())
        .await
        .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        insert_ledger(
            &mut tx,
            LedgerInsert {
                user_id,
                entry_type: "reservation",
                amount,
                balance_after: wallet.balance,
                reference_type: "reservation",
                reference_id: Some(reservation_id),
                idempotency_key: &idempotency_key,
            },
        )
        .await?;
        tx.commit()
            .await
            .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        Ok(Some(reservation))
    }

    pub async fn release_reservation(
        &self,
        user_id: Uuid,
        reservation_id: Uuid,
    ) -> Result<bool, BillingError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        let wallet = lock_wallet(&mut tx, user_id).await?;
        let reservation = lock_reservation(&mut tx, reservation_id, user_id).await?;
        if reservation.status != "pending" {
            tx.rollback()
                .await
                .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
            return Ok(false);
        }
        let reserved_balance = wallet.reserved_balance - reservation.amount;
        if reserved_balance < Decimal::ZERO {
            return Err(BillingError::WalletLedgerIntegrityError(
                "reserved balance would become negative".into(),
            ));
        }
        sqlx::query!(
            "UPDATE wallets SET reserved_balance=$2, updated_at=now() WHERE user_id=$1",
            user_id,
            reserved_balance,
        )
        .execute(tx.as_mut())
        .await
        .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        sqlx::query!(
            "UPDATE reservations SET status='released', released_at=now() WHERE id=$1",
            reservation_id,
        )
        .execute(tx.as_mut())
        .await
        .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        let release_key = format!("release:{reservation_id}");
        insert_ledger(
            &mut tx,
            LedgerInsert {
                user_id,
                entry_type: "reservation_release",
                amount: reservation.amount,
                balance_after: wallet.balance,
                reference_type: "reservation",
                reference_id: Some(reservation_id),
                idempotency_key: &release_key,
            },
        )
        .await?;
        tx.commit()
            .await
            .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        Ok(true)
    }

    pub async fn confirm_debit(
        &self,
        user_id: Uuid,
        reservation_id: Uuid,
        request_id: Uuid,
        model_catalog_id: Uuid,
        actual_amount: Decimal,
        aws_cost: Decimal,
    ) -> Result<Wallet, BillingError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        let wallet = lock_wallet(&mut tx, user_id).await?;
        let reservation = lock_reservation(&mut tx, reservation_id, user_id).await?;
        if reservation.status != "pending" {
            return Err(BillingError::ReservationNotPending);
        }
        if actual_amount > reservation.amount {
            return Err(BillingError::Validation(format!(
                "actual_amount {} exceeds reservation amount {}",
                actual_amount, reservation.amount
            )));
        }
        let reserved_balance = wallet.reserved_balance - reservation.amount;
        if reserved_balance < Decimal::ZERO {
            return Err(BillingError::WalletLedgerIntegrityError(
                "reserved balance would become negative".into(),
            ));
        }
        let new_balance = wallet.balance - actual_amount;
        if new_balance < Decimal::ZERO {
            return Err(BillingError::InsufficientBalance);
        }
        sqlx::query!(
            "UPDATE wallets SET balance=$2,reserved_balance=$3,updated_at=now() WHERE user_id=$1",
            user_id,
            new_balance,
            reserved_balance
        )
        .execute(tx.as_mut())
        .await
        .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        sqlx::query!("UPDATE reservations SET status='confirmed', request_id=$2, model_catalog_id=$3, actual_amount=$4, aws_cost=$5, confirmed_at=now() WHERE id=$1",
            reservation_id,
            request_id,
            model_catalog_id,
            actual_amount,
            aws_cost)
            .execute(tx.as_mut())
            .await
            .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        let release_key = format!("confirm-release:{reservation_id}");
        insert_ledger(
            &mut tx,
            LedgerInsert {
                user_id,
                entry_type: "reservation_release",
                amount: reservation.amount,
                balance_after: wallet.balance,
                reference_type: "reservation",
                reference_id: Some(reservation_id),
                idempotency_key: &release_key,
            },
        )
        .await?;
        let debit_key = format!("debit:{request_id}");
        insert_ledger(
            &mut tx,
            LedgerInsert {
                user_id,
                entry_type: "debit",
                amount: actual_amount,
                balance_after: new_balance,
                reference_type: "usage_event",
                reference_id: Some(request_id),
                idempotency_key: &debit_key,
            },
        )
        .await?;
        insert_transaction(
            &mut tx,
            TransactionInsert {
                user_id,
                transaction_type: "charge",
                amount: actual_amount,
                currency: &wallet.currency,
                provider: None,
                provider_reference: None,
                status: "succeeded",
            },
        )
        .await?;
        let debited = EventEnvelope::new(
            EVENT_WALLET_DEBITED,
            WalletDebited {
                user_id,
                amount: actual_amount,
                currency: wallet.currency.clone(),
                balance_after: new_balance,
                usage_event_id: request_id,
            },
        );
        enqueue_in_transaction(&mut tx, "event_outbox", EVENT_WALLET_DEBITED, &debited)
            .await
            .map_err(|error| BillingError::DependencyUnavailable(error.to_string()))?;
        let available_balance = new_balance - reserved_balance;
        if available_balance <= wallet.low_balance_threshold {
            let low_balance = EventEnvelope::new(
                EVENT_WALLET_LOW_BALANCE,
                WalletLowBalance {
                    user_id,
                    currency: wallet.currency.clone(),
                    current_balance: available_balance,
                    threshold: wallet.low_balance_threshold,
                },
            );
            enqueue_in_transaction(
                &mut tx,
                "event_outbox",
                EVENT_WALLET_LOW_BALANCE,
                &low_balance,
            )
            .await
            .map_err(|error| BillingError::DependencyUnavailable(error.to_string()))?;
        }
        tx.commit()
            .await
            .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        self.get_wallet(user_id)
            .await?
            .ok_or(BillingError::WalletNotFound)
    }

    pub async fn credit_wallet(
        &self,
        input: CreditWalletInput<'_>,
    ) -> Result<Wallet, BillingError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        ensure_wallet(
            &mut tx,
            input.user_id,
            input.currency,
            input.low_balance_threshold,
        )
        .await?;
        let wallet = lock_wallet(&mut tx, input.user_id).await?;
        if wallet.currency != input.currency {
            return Err(BillingError::Validation(format!(
                "wallet currency {} does not match payment currency {}",
                wallet.currency, input.currency
            )));
        }
        let ledger_key = format!("payment:{}", input.payment_transaction_id);
        let inserted = insert_ledger(
            &mut tx,
            LedgerInsert {
                user_id: input.user_id,
                entry_type: "credit",
                amount: input.amount,
                balance_after: wallet.balance + input.amount,
                reference_type: "topup",
                reference_id: Some(input.payment_transaction_id),
                idempotency_key: &ledger_key,
            },
        )
        .await?;
        if !inserted {
            tx.commit()
                .await
                .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
            return self
                .get_wallet(input.user_id)
                .await?
                .ok_or(BillingError::WalletNotFound);
        }
        let new_balance = wallet.balance + input.amount;
        sqlx::query!(
            "UPDATE wallets SET balance=$2, updated_at=now() WHERE user_id=$1",
            input.user_id,
            new_balance
        )
        .execute(tx.as_mut())
        .await
        .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        insert_transaction(
            &mut tx,
            TransactionInsert {
                user_id: input.user_id,
                transaction_type: "topup",
                amount: input.amount,
                currency: input.currency,
                provider: Some(input.provider),
                provider_reference: Some(input.provider_reference),
                status: "succeeded",
            },
        )
        .await?;
        tx.commit()
            .await
            .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        self.get_wallet(input.user_id)
            .await?
            .ok_or(BillingError::WalletNotFound)
    }

    pub async fn adjust_wallet(
        &self,
        user_id: Uuid,
        amount: Decimal,
        currency: &str,
        reason: &str,
        idempotency_key: &str,
        low_balance_threshold: Decimal,
    ) -> Result<Wallet, BillingError> {
        tracing::info!(%user_id, %amount, %currency, %reason, "adjusting wallet");
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        ensure_wallet(&mut tx, user_id, currency, low_balance_threshold).await?;
        let wallet = lock_wallet(&mut tx, user_id).await?;
        let entry_type = if amount.is_sign_negative() {
            "debit"
        } else {
            "credit"
        };
        let inserted = insert_ledger(
            &mut tx,
            LedgerInsert {
                user_id,
                entry_type,
                amount: amount.abs(),
                balance_after: wallet.balance + amount,
                reference_type: "adjustment",
                reference_id: None,
                idempotency_key,
            },
        )
        .await?;
        if !inserted {
            tx.commit()
                .await
                .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
            return self
                .get_wallet(user_id)
                .await?
                .ok_or(BillingError::WalletNotFound);
        }
        let new_balance = wallet.balance + amount;
        if new_balance < Decimal::ZERO {
            return Err(BillingError::InsufficientBalance);
        }
        sqlx::query!(
            "UPDATE wallets SET balance=$2, updated_at=now() WHERE user_id=$1",
            user_id,
            new_balance
        )
        .execute(tx.as_mut())
        .await
        .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        insert_transaction(
            &mut tx,
            TransactionInsert {
                user_id,
                transaction_type: "adjustment",
                amount,
                currency,
                provider: None,
                provider_reference: None,
                status: "succeeded",
            },
        )
        .await?;
        tx.commit()
            .await
            .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        self.get_wallet(user_id)
            .await?
            .ok_or(BillingError::WalletNotFound)
    }

    pub async fn reconcile_wallet(&self, user_id: Uuid) -> Result<Wallet, BillingError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        lock_wallet(&mut tx, user_id).await?;
        let calculated = sqlx::query_scalar!(
            "SELECT COALESCE(SUM(CASE WHEN entry_type='credit' THEN amount WHEN entry_type='debit' THEN -amount ELSE 0 END), 0) FROM wallet_ledger WHERE user_id=$1",
            user_id,
        )
        .fetch_one(tx.as_mut())
        .await
        .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        let reserved = sqlx::query_scalar!(
            "SELECT COALESCE(SUM(amount), 0) FROM reservations WHERE user_id=$1 AND status='pending'",
            user_id,
        )
        .fetch_one(tx.as_mut())
        .await
        .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        sqlx::query!(
            "UPDATE wallets SET balance=$2,reserved_balance=$3,updated_at=now() WHERE user_id=$1",
            user_id,
            calculated,
            reserved
        )
        .execute(tx.as_mut())
        .await
        .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        tx.commit()
            .await
            .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        self.get_wallet(user_id)
            .await?
            .ok_or(BillingError::WalletNotFound)
    }

    pub async fn expire_overdue_reservations(&self, limit: i64) -> Result<u64, BillingError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        let reservations = sqlx::query_as::<_, Reservation>(
            "SELECT * FROM reservations WHERE status='pending' AND expires_at < now() ORDER BY expires_at ASC LIMIT $1 FOR UPDATE",
        )
        .bind(limit.clamp(1, 1000))
        .fetch_all(tx.as_mut())
        .await
        .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        let mut released = 0;
        for reservation in &reservations {
            let wallet = lock_wallet(&mut tx, reservation.user_id).await?;
            let reserved_balance = wallet.reserved_balance - reservation.amount;
            if reserved_balance < Decimal::ZERO {
                continue;
            }
            sqlx::query!(
                "UPDATE wallets SET reserved_balance=$2, updated_at=now() WHERE user_id=$1",
                reservation.user_id,
                reserved_balance,
            )
            .execute(tx.as_mut())
            .await
            .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
            sqlx::query!(
                "UPDATE reservations SET status='expired', released_at=now() WHERE id=$1",
                reservation.id,
            )
            .execute(tx.as_mut())
            .await
            .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
            let release_key = format!("release:{}", reservation.id);
            insert_ledger(
                &mut tx,
                LedgerInsert {
                    user_id: reservation.user_id,
                    entry_type: "credit",
                    amount: reservation.amount,
                    balance_after: wallet.balance,
                    reference_type: "expired_reservation",
                    reference_id: Some(reservation.id),
                    idempotency_key: &release_key,
                },
            )
            .await?;
            released += 1;
        }
        tx.commit()
            .await
            .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        Ok(released)
    }

    pub async fn log_audit(
        &self,
        actor_type: &str,
        actor_id: Option<Uuid>,
        action: &str,
        resource_type: &str,
        resource_id: Option<Uuid>,
        metadata: serde_json::Value,
    ) -> Result<(), BillingError> {
        let metadata_str = serde_json::to_string(&metadata)
            .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
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
            .map_err(BillingError::DependencyUnavailable)?;
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
        .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }

    pub async fn active_webhook_endpoints(
        &self,
        user_id: Uuid,
        event_type: &str,
    ) -> Result<Vec<WebhookEndpoint>, BillingError> {
        sqlx::query_as("SELECT id,user_id,url,signing_secret_encrypted,subscribed_events,is_active FROM webhook_endpoints WHERE user_id=$1 AND is_active=true AND $2 = ANY(subscribed_events)")
            .bind(user_id)
            .bind(event_type)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))
    }

    pub async fn record_webhook_delivery(
        &self,
        endpoint_id: Uuid,
        event_type: &str,
        payload: serde_json::Value,
        status: &str,
        attempts: i16,
    ) -> Result<(), BillingError> {
        sqlx::query!("INSERT INTO webhook_deliveries (id,webhook_endpoint_id,event_type,payload,status,attempts,last_attempt_at) VALUES ($1,$2,$3,$4,$5,$6,now())",
            Uuid::now_v7(),
            endpoint_id,
            event_type,
            payload,
            status,
            attempts)
            .execute(&self.pool)
            .await
            .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }
}

async fn ensure_wallet(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    currency: &str,
    low_balance_threshold: Decimal,
) -> Result<(), BillingError> {
    sqlx::query!("INSERT INTO wallets (user_id,currency,low_balance_threshold) VALUES ($1,$2,$3) ON CONFLICT (user_id) DO NOTHING",
        user_id,
        currency,
        low_balance_threshold)
        .execute(tx.as_mut())
        .await
        .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
    Ok(())
}

async fn lock_wallet(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<Wallet, BillingError> {
    sqlx::query_as("SELECT * FROM wallets WHERE user_id=$1 FOR UPDATE")
        .bind(user_id)
        .fetch_optional(tx.as_mut())
        .await
        .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?
        .ok_or(BillingError::WalletNotFound)
}

async fn lock_reservation(
    tx: &mut Transaction<'_, Postgres>,
    reservation_id: Uuid,
    user_id: Uuid,
) -> Result<Reservation, BillingError> {
    sqlx::query_as("SELECT * FROM reservations WHERE id=$1 AND user_id=$2 FOR UPDATE")
        .bind(reservation_id)
        .bind(user_id)
        .fetch_optional(tx.as_mut())
        .await
        .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?
        .ok_or(BillingError::ReservationNotFound)
}

async fn insert_ledger(
    tx: &mut Transaction<'_, Postgres>,
    input: LedgerInsert<'_>,
) -> Result<bool, BillingError> {
    let result = sqlx::query!(
        "INSERT INTO wallet_ledger (id,user_id,entry_type,amount,balance_after,reference_type,reference_id,idempotency_key) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (idempotency_key) DO NOTHING",
        Uuid::now_v7(),
        input.user_id,
        input.entry_type,
        input.amount,
        input.balance_after,
        input.reference_type,
        input.reference_id,
        input.idempotency_key)
        .execute(tx.as_mut())
        .await
        .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
    Ok(result.rows_affected() > 0)
}

async fn insert_transaction(
    tx: &mut Transaction<'_, Postgres>,
    input: TransactionInsert<'_>,
) -> Result<(), BillingError> {
    sqlx::query!("INSERT INTO transactions (id,user_id,type,amount,currency,provider,provider_reference,status) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        Uuid::now_v7(),
        input.user_id,
        input.transaction_type,
        input.amount,
        input.currency,
        input.provider,
        input.provider_reference,
        input.status)
        .execute(tx.as_mut())
        .await
        .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
    Ok(())
}
