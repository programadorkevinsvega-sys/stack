use crate::application::ports::{AuditLogInput, AuditLogger};
use crate::domain::entities::{PaymentMethod, PaymentProviderEvent, PaymentTransaction};
use crate::domain::errors::PaymentError;
use crate::domain::repositories::{
    PaymentMethodRepository, PaymentProviderEventRepository, PaymentTransactionRepository,
};
use async_trait::async_trait;
use events::{
    outbox::enqueue_in_transaction, EventEnvelope, PaymentFailed, PaymentSucceeded,
    EVENT_PAYMENT_FAILED, EVENT_PAYMENT_SUCCEEDED,
};
use sqlx::PgPool;
use uuid::Uuid;

pub struct PgPaymentMethodRepository {
    pub pool: PgPool,
}

pub struct PgPaymentProviderEventRepository {
    pub pool: PgPool,
}

pub struct PgPaymentTransactionRepository {
    pub pool: PgPool,
}

pub struct PgAuditLogger {
    pub pool: PgPool,
    pub signer: common::AuditSigner,
}

#[async_trait]
impl PaymentMethodRepository for PgPaymentMethodRepository {
    async fn save(&self, value: &PaymentMethod) -> Result<(), PaymentError> {
        sqlx::query!("INSERT INTO payment_methods (id,user_id,provider,provider_payment_method_id,brand,last4,exp_month,exp_year,is_default,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
            value.id,
            value.user_id,
            &value.provider,
            &value.provider_payment_method_id,
            value.brand.as_deref(),
            value.last4.as_deref(),
            value.exp_month,
            value.exp_year,
            value.is_default,
            value.created_at)
            .execute(&self.pool)
            .await
            .map_err(|e| PaymentError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<PaymentMethod>, PaymentError> {
        sqlx::query_as("SELECT * FROM payment_methods WHERE id=$1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| PaymentError::DependencyUnavailable(e.to_string()))
    }

    async fn find_by_user(&self, user_id: Uuid) -> Result<Vec<PaymentMethod>, PaymentError> {
        sqlx::query_as(
            "SELECT * FROM payment_methods WHERE user_id=$1 ORDER BY is_default DESC, created_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| PaymentError::DependencyUnavailable(e.to_string()))
    }

    async fn set_default(&self, user_id: Uuid, id: Uuid) -> Result<(), PaymentError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| PaymentError::DependencyUnavailable(e.to_string()))?;
        let method = sqlx::query_as::<_, PaymentMethod>(
            "SELECT * FROM payment_methods WHERE id=$1 FOR UPDATE",
        )
        .bind(id)
        .fetch_optional(tx.as_mut())
        .await
        .map_err(|e| PaymentError::DependencyUnavailable(e.to_string()))?;
        match method {
            Some(value) if value.user_id == user_id => {}
            _ => return Err(PaymentError::PaymentMethodNotFound),
        }
        sqlx::query!(
            "UPDATE payment_methods SET is_default=false WHERE user_id=$1",
            user_id
        )
        .execute(tx.as_mut())
        .await
        .map_err(|e| PaymentError::DependencyUnavailable(e.to_string()))?;
        sqlx::query!("UPDATE payment_methods SET is_default=true WHERE id=$1", id)
            .execute(tx.as_mut())
            .await
            .map_err(|e| PaymentError::DependencyUnavailable(e.to_string()))?;
        tx.commit()
            .await
            .map_err(|e| PaymentError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, user_id: Uuid, id: Uuid) -> Result<(), PaymentError> {
        let result = sqlx::query!(
            "DELETE FROM payment_methods WHERE id=$1 AND user_id=$2",
            id,
            user_id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| PaymentError::DependencyUnavailable(e.to_string()))?;
        if result.rows_affected() == 0 {
            return Err(PaymentError::PaymentMethodNotFound);
        }
        Ok(())
    }
}

#[async_trait]
impl PaymentProviderEventRepository for PgPaymentProviderEventRepository {
    async fn save(&self, value: &PaymentProviderEvent) -> Result<bool, PaymentError> {
        let result = sqlx::query!(
            "INSERT INTO payment_provider_events (id,provider,provider_event_id,event_type,payload,processed_at,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (provider, provider_event_id) DO NOTHING",
            value.id,
            &value.provider,
            &value.provider_event_id,
            &value.event_type,
            &value.payload,
            value.processed_at,
            value.created_at)
            .execute(&self.pool)
            .await
            .map_err(|e| PaymentError::DependencyUnavailable(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    async fn find_by_provider_event_id(
        &self,
        provider: &str,
        provider_event_id: &str,
    ) -> Result<Option<PaymentProviderEvent>, PaymentError> {
        sqlx::query_as(
            "SELECT * FROM payment_provider_events WHERE provider=$1 AND provider_event_id=$2",
        )
        .bind(provider)
        .bind(provider_event_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| PaymentError::DependencyUnavailable(e.to_string()))
    }

    async fn mark_processed(&self, id: Uuid) -> Result<(), PaymentError> {
        sqlx::query!(
            "UPDATE payment_provider_events SET processed_at=now() WHERE id=$1",
            id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| PaymentError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl PaymentTransactionRepository for PgPaymentTransactionRepository {
    async fn save(&self, value: &PaymentTransaction) -> Result<(), PaymentError> {
        sqlx::query!("INSERT INTO payment_transactions (id,user_id,provider,provider_reference,kind,status,amount,currency,idempotency_key,failure_reason,metadata,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)",
            value.id,
            value.user_id,
            &value.provider,
            &value.provider_reference,
            &value.kind,
            &value.status,
            value.amount,
            &value.currency,
            value.idempotency_key.as_deref(),
            value.failure_reason.as_deref(),
            &value.metadata,
            value.created_at,
            value.updated_at)
            .execute(&self.pool)
            .await
            .map_err(|e| PaymentError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<PaymentTransaction>, PaymentError> {
        sqlx::query_as("SELECT * FROM payment_transactions WHERE id=$1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| PaymentError::DependencyUnavailable(e.to_string()))
    }

    async fn find_by_provider_reference(
        &self,
        provider: &str,
        provider_reference: &str,
    ) -> Result<Option<PaymentTransaction>, PaymentError> {
        sqlx::query_as(
            "SELECT * FROM payment_transactions WHERE provider=$1 AND provider_reference=$2",
        )
        .bind(provider)
        .bind(provider_reference)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| PaymentError::DependencyUnavailable(e.to_string()))
    }

    async fn find_by_idempotency_key(
        &self,
        provider: &str,
        idempotency_key: &str,
    ) -> Result<Option<PaymentTransaction>, PaymentError> {
        sqlx::query_as(
            "SELECT * FROM payment_transactions WHERE provider=$1 AND idempotency_key=$2",
        )
        .bind(provider)
        .bind(idempotency_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| PaymentError::DependencyUnavailable(e.to_string()))
    }

    async fn update(&self, value: &PaymentTransaction) -> Result<(), PaymentError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| PaymentError::DependencyUnavailable(e.to_string()))?;
        let previous_status = sqlx::query_scalar::<_, String>(
            "SELECT status FROM payment_transactions WHERE id=$1 FOR UPDATE",
        )
        .bind(value.id)
        .fetch_optional(tx.as_mut())
        .await
        .map_err(|e| PaymentError::DependencyUnavailable(e.to_string()))?
        .ok_or(PaymentError::PaymentProviderEventNotFound)?;
        sqlx::query!("UPDATE payment_transactions SET provider_reference=$2,status=$3,amount=$4,currency=$5,failure_reason=$6,metadata=$7,updated_at=$8 WHERE id=$1",
            value.id,
            &value.provider_reference,
            &value.status,
            value.amount,
            &value.currency,
            value.failure_reason.as_deref(),
            &value.metadata,
            value.updated_at)
            .execute(tx.as_mut())
            .await
            .map_err(|e| PaymentError::DependencyUnavailable(e.to_string()))?;
        if previous_status != value.status && value.status == "succeeded" {
            let provider_reference = value
                .metadata
                .get("capture_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(&value.provider_reference)
                .to_string();
            let envelope = EventEnvelope::new(
                EVENT_PAYMENT_SUCCEEDED,
                PaymentSucceeded {
                    transaction_id: value.id,
                    user_id: value.user_id,
                    amount: value.amount,
                    currency: value.currency.clone(),
                    provider: value.provider.clone(),
                    provider_reference,
                    kind: value.kind.clone(),
                    metadata: value
                        .metadata
                        .as_object()
                        .map(|o| {
                            o.iter()
                                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                                .collect()
                        })
                        .unwrap_or_default(),
                },
            );
            enqueue_in_transaction(&mut tx, "event_outbox", EVENT_PAYMENT_SUCCEEDED, &envelope)
                .await
                .map_err(|error| PaymentError::DependencyUnavailable(error.to_string()))?;
        } else if previous_status != value.status && value.status == "failed" {
            let envelope = EventEnvelope::new(
                EVENT_PAYMENT_FAILED,
                PaymentFailed {
                    transaction_id: value.id,
                    user_id: value.user_id,
                    amount: value.amount,
                    currency: value.currency.clone(),
                    provider: value.provider.clone(),
                    reason: value
                        .failure_reason
                        .clone()
                        .unwrap_or_else(|| "payment_failed".to_string()),
                    kind: value.kind.clone(),
                    metadata: value
                        .metadata
                        .as_object()
                        .map(|o| {
                            o.iter()
                                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                                .collect()
                        })
                        .unwrap_or_default(),
                },
            );
            enqueue_in_transaction(&mut tx, "event_outbox", EVENT_PAYMENT_FAILED, &envelope)
                .await
                .map_err(|error| PaymentError::DependencyUnavailable(error.to_string()))?;
        }
        tx.commit()
            .await
            .map_err(|e| PaymentError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl AuditLogger for PgAuditLogger {
    async fn log(&self, input: AuditLogInput) -> Result<(), PaymentError> {
        let metadata_str = serde_json::to_string(&input.metadata)
            .map_err(|e| PaymentError::DependencyUnavailable(e.to_string()))?;
        let sig = self
            .signer
            .sign(
                &input.actor_type,
                input.actor_id,
                &input.action,
                &input.resource_type,
                input.resource_id,
                Some(&metadata_str),
            )
            .map_err(PaymentError::DependencyUnavailable)?;
        sqlx::query("INSERT INTO audit_logs (id,actor_type,actor_id,action,resource_type,resource_id,metadata,ip_address,signature,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8::inet,$9,now())")
            .bind(Uuid::now_v7())
            .bind(&input.actor_type)
            .bind(input.actor_id)
            .bind(&input.action)
            .bind(&input.resource_type)
            .bind(input.resource_id)
            .bind(&input.metadata)
            .bind(input.ip_address)
            .bind(sig)
            .execute(&self.pool)
            .await
            .map_err(|e| PaymentError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }
}
