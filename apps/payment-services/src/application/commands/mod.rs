use crate::application::ports::{
    AuditLogInput, AuditLogger, CreateOrderInput, PayPalWebhookHeaders, PaymentProvider,
};
use crate::domain::entities::{
    CapturedPayment, PaymentMethod, PaymentProviderEvent, PaymentTransaction, ProviderOrder,
    ProviderPaymentToken, ProviderVaultSetup, TopUpContext,
};
use crate::domain::errors::PaymentError;
use crate::domain::repositories::{
    PaymentMethodRepository, PaymentProviderEventRepository, PaymentTransactionRepository,
};
use chrono::Utc;
use rust_decimal::Decimal;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

const PROVIDER_PAYPAL: &str = "paypal";

fn validate_provider(provider: &str) -> Result<(), PaymentError> {
    match provider {
        "paypal" => Ok(()),
        _ => Err(PaymentError::Validation(
            "unsupported payment provider".into(),
        )),
    }
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn top_up_context(payload: &Value) -> Result<TopUpContext, PaymentError> {
    serde_json::from_value(payload.clone()).map_err(|e| PaymentError::Validation(e.to_string()))
}

async fn save_event_once(
    repo: &Arc<dyn PaymentProviderEventRepository>,
    provider: &str,
    provider_event_id: String,
    event_type: String,
    payload: Value,
    processed: bool,
) -> Result<Option<PaymentProviderEvent>, PaymentError> {
    let event = PaymentProviderEvent {
        id: Uuid::now_v7(),
        provider: provider.to_string(),
        provider_event_id,
        event_type,
        payload,
        processed_at: processed.then(Utc::now),
        created_at: Utc::now(),
    };
    let inserted = repo.save(&event).await?;
    if inserted {
        Ok(Some(event))
    } else {
        Ok(None)
    }
}

async fn publish_success_once(
    repo: &Arc<dyn PaymentProviderEventRepository>,
    transactions: &Arc<dyn PaymentTransactionRepository>,
    context: &TopUpContext,
    capture: &CapturedPayment,
) -> Result<(), PaymentError> {
    let synthetic_id = format!("paypal.capture.completed:{}", capture.order_id);
    let _ = save_event_once(
        repo,
        PROVIDER_PAYPAL,
        synthetic_id,
        "paypal.capture.completed".into(),
        json!({
            "user_id": context.user_id,
            "amount": capture.amount,
            "currency": capture.currency,
            "order_id": capture.order_id,
            "capture_id": capture.provider_reference,
            "status": capture.status
        }),
        true,
    )
    .await?;
    let mut transaction = transactions
        .find_by_id(context.transaction_id)
        .await?
        .ok_or(PaymentError::PaymentProviderEventNotFound)?;
    if transaction.status != "succeeded" {
        transaction.status = "succeeded".into();
        transaction.provider_reference = capture.order_id.clone();
        transaction.amount = capture.amount;
        transaction.currency = capture.currency.clone();
        transaction.failure_reason = None;
        transaction.metadata = json!({
            "capture_id": capture.provider_reference,
            "order_id": capture.order_id,
            "status": capture.status
        });
        transaction.updated_at = Utc::now();
        transactions.update(&transaction).await?;
    }
    Ok(())
}

pub struct CreateTopUp {
    pub provider: Arc<dyn PaymentProvider>,
    pub events: Arc<dyn PaymentProviderEventRepository>,
    pub transactions: Arc<dyn PaymentTransactionRepository>,
    pub audit: Arc<dyn AuditLogger>,
}

pub struct CreateTopUpInput {
    pub user_id: Uuid,
    pub amount: Decimal,
    pub currency: String,
    pub idempotency_key: String,
    pub return_url: String,
    pub cancel_url: String,
    pub kind: String,
    pub reference_id: String,
}

pub struct ChargePayPalVault {
    pub provider: Arc<dyn PaymentProvider>,
    pub methods: Arc<dyn PaymentMethodRepository>,
    pub transactions: Arc<dyn PaymentTransactionRepository>,
    pub provider_events: Arc<dyn PaymentProviderEventRepository>,
    pub audit: Arc<dyn AuditLogger>,
}

impl ChargePayPalVault {
    pub async fn execute(
        &self,
        user_id: Uuid,
        amount: Decimal,
        currency: String,
        idempotency_key: String,
        reference_id: String,
    ) -> Result<CapturedPayment, PaymentError> {
        if idempotency_key.trim().is_empty() || amount <= Decimal::ZERO {
            return Err(PaymentError::Validation(
                "valid amount and idempotency_key are required".into(),
            ));
        }
        if let Some(existing) = self
            .transactions
            .find_by_idempotency_key(PROVIDER_PAYPAL, &idempotency_key)
            .await?
        {
            if existing.status == "succeeded" {
                return Ok(CapturedPayment {
                    provider_reference: existing.provider_reference.clone(),
                    order_id: existing.provider_reference,
                    amount: existing.amount,
                    currency: existing.currency,
                    status: "COMPLETED".into(),
                });
            }
        }
        let method = self
            .methods
            .find_by_user(user_id)
            .await?
            .into_iter()
            .find(|m| m.is_default && m.provider == PROVIDER_PAYPAL)
            .ok_or_else(|| {
                PaymentError::PaymentFailed(
                    "a default PayPal Vault payment method is required".into(),
                )
            })?;
        let capture = self
            .provider
            .charge_vault_token(
                user_id,
                &method.provider_payment_method_id,
                amount,
                &currency,
                &idempotency_key,
            )
            .await?;
        let transaction = PaymentTransaction {
            id: Uuid::now_v7(),
            user_id,
            provider: PROVIDER_PAYPAL.into(),
            provider_reference: capture.provider_reference.clone(),
            kind: "subscription_charge".into(),
            status: "succeeded".into(),
            amount: capture.amount,
            currency: capture.currency.clone(),
            idempotency_key: Some(idempotency_key),
            failure_reason: None,
            metadata: json!({"reference_id": reference_id, "order_id": capture.order_id}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        self.transactions.save(&transaction).await?;
        let context = TopUpContext {
            user_id,
            transaction_id: transaction.id,
            amount: capture.amount,
            currency: capture.currency.clone(),
            order_id: capture.order_id.clone(),
            kind: "subscription_charge".into(),
            reference_id: reference_id.clone(),
        };
        save_event_once(
            &self.provider_events,
            PROVIDER_PAYPAL,
            capture.order_id.clone(),
            "paypal.order.created".into(),
            serde_json::to_value(context).map_err(|e| PaymentError::Validation(e.to_string()))?,
            true,
        )
        .await?;
        self.audit.log(AuditLogInput { actor_type: "system".into(), actor_id: None, action: "payment.paypal.vault_charge.succeeded".into(), resource_type: "payment_transaction".into(), resource_id: Some(transaction.id), metadata: json!({"user_id": user_id, "reference_id": reference_id, "provider_reference": capture.provider_reference}), ip_address: None }).await?;
        Ok(capture)
    }
}

impl CreateTopUp {
    pub async fn execute(&self, input: CreateTopUpInput) -> Result<ProviderOrder, PaymentError> {
        let CreateTopUpInput {
            user_id,
            amount,
            currency,
            idempotency_key,
            return_url,
            cancel_url,
            kind,
            reference_id,
        } = input;
        if idempotency_key.trim().is_empty() {
            return Err(PaymentError::Validation(
                "idempotency_key is required".into(),
            ));
        }
        if let Some(existing) = self
            .transactions
            .find_by_idempotency_key(PROVIDER_PAYPAL, &idempotency_key)
            .await?
        {
            return Ok(ProviderOrder {
                provider_reference: existing.provider_reference,
                approval_url: existing
                    .metadata
                    .get("approval_url")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                status: existing.status,
            });
        }
        let order = self
            .provider
            .create_order(CreateOrderInput {
                user_id,
                amount,
                currency: currency.clone(),
                idempotency_key: idempotency_key.clone(),
                return_url,
                cancel_url,
            })
            .await?;
        let transaction = PaymentTransaction {
            id: Uuid::now_v7(),
            user_id,
            provider: PROVIDER_PAYPAL.into(),
            provider_reference: order.provider_reference.clone(),
            kind: if kind.trim().is_empty() {
                "top_up".into()
            } else {
                kind.clone()
            },
            status: "requires_approval".into(),
            amount,
            currency: currency.trim().to_uppercase(),
            idempotency_key: Some(idempotency_key),
            failure_reason: None,
            metadata: json!({
                "approval_url": order.approval_url,
                "paypal_status": order.status,
                "kind": kind,
                "reference_id": reference_id
            }),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        self.transactions.save(&transaction).await?;
        let context = TopUpContext {
            user_id,
            transaction_id: transaction.id,
            amount,
            currency: currency.trim().to_uppercase(),
            order_id: order.provider_reference.clone(),
            kind: transaction.kind.clone(),
            reference_id: transaction
                .metadata
                .get("reference_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        };
        save_event_once(
            &self.events,
            PROVIDER_PAYPAL,
            order.provider_reference.clone(),
            "paypal.order.created".into(),
            serde_json::to_value(context).map_err(|e| PaymentError::Validation(e.to_string()))?,
            false,
        )
        .await?;
        self.audit
            .log(AuditLogInput {
                actor_type: "user".into(),
                actor_id: Some(user_id),
                action: if transaction.kind == "subscription_charge" {
                    "payment.subscription.created".into()
                } else {
                    "payment.top_up.created".into()
                },
                resource_type: "payment_transaction".into(),
                resource_id: Some(transaction.id),
                metadata: json!({
                    "provider": PROVIDER_PAYPAL,
                    "amount": amount,
                    "currency": currency,
                    "provider_reference": order.provider_reference
                }),
                ip_address: None,
            })
            .await?;
        Ok(order)
    }
}

pub struct CapturePayPalOrder {
    pub provider: Arc<dyn PaymentProvider>,
    pub provider_events: Arc<dyn PaymentProviderEventRepository>,
    pub transactions: Arc<dyn PaymentTransactionRepository>,
    pub audit: Arc<dyn AuditLogger>,
}

impl CapturePayPalOrder {
    pub async fn execute(
        &self,
        user_id: Uuid,
        order_id: String,
    ) -> Result<CapturedPayment, PaymentError> {
        let order_event = self
            .provider_events
            .find_by_provider_event_id(PROVIDER_PAYPAL, &order_id)
            .await?
            .ok_or(PaymentError::PaymentProviderEventNotFound)?;
        let context = top_up_context(&order_event.payload)?;
        if context.user_id != user_id {
            return Err(PaymentError::Validation(
                "paypal order does not belong to user".into(),
            ));
        }
        let capture_ref = format!("capture_{order_id}");
        if let Some(existing) = self
            .provider_events
            .find_by_provider_event_id(PROVIDER_PAYPAL, &capture_ref)
            .await?
        {
            if existing.processed_at.is_some() {
                let capture = serde_json::from_value::<CapturedPayment>(existing.payload.clone())
                    .map_err(|e| PaymentError::Validation(e.to_string()))?;
                return Ok(capture);
            }
        }
        let capture = self.provider.capture_order(&order_id).await?;
        let capture_ref = format!("capture_{order_id}");
        save_event_once(
            &self.provider_events,
            PROVIDER_PAYPAL,
            capture_ref,
            "paypal.capture.completed".into(),
            serde_json::to_value(&capture).map_err(|e| PaymentError::Validation(e.to_string()))?,
            true,
        )
        .await?;
        publish_success_once(
            &self.provider_events,
            &self.transactions,
            &context,
            &capture,
        )
        .await?;
        self.audit
            .log(AuditLogInput {
                actor_type: "user".into(),
                actor_id: Some(user_id),
                action: "payment.paypal_order.captured".into(),
                resource_type: "payment_transaction".into(),
                resource_id: Some(context.transaction_id),
                metadata: json!({
                    "order_id": order_id,
                    "capture_id": capture.provider_reference
                }),
                ip_address: None,
            })
            .await?;
        Ok(capture)
    }
}

pub struct AttachPaymentMethod {
    pub repo: Arc<dyn PaymentMethodRepository>,
    pub audit: Arc<dyn AuditLogger>,
}

pub struct AttachPaymentMethodInput {
    pub user_id: Uuid,
    pub provider: String,
    pub provider_payment_method_id: String,
    pub brand: Option<String>,
    pub last4: Option<String>,
    pub exp_month: Option<i32>,
    pub exp_year: Option<i32>,
    pub set_default: bool,
}

impl AttachPaymentMethod {
    pub async fn execute(
        &self,
        input: AttachPaymentMethodInput,
    ) -> Result<PaymentMethod, PaymentError> {
        validate_provider(&input.provider)?;
        if input.provider_payment_method_id.trim().is_empty() {
            return Err(PaymentError::Validation(
                "provider_payment_method_id is required".into(),
            ));
        }
        let method = PaymentMethod {
            id: Uuid::now_v7(),
            user_id: input.user_id,
            provider: input.provider,
            provider_payment_method_id: input.provider_payment_method_id,
            brand: clean_optional(input.brand),
            last4: clean_optional(input.last4),
            exp_month: input.exp_month.map(|v| v as i16),
            exp_year: input.exp_year.map(|v| v as i16),
            is_default: input.set_default,
            created_at: Utc::now(),
        };
        self.repo.save(&method).await?;
        if input.set_default {
            self.repo.set_default(input.user_id, method.id).await?;
        }
        self.audit
            .log(AuditLogInput {
                actor_type: "user".into(),
                actor_id: Some(input.user_id),
                action: "payment_method.attached".into(),
                resource_type: "payment_method".into(),
                resource_id: Some(method.id),
                metadata: json!({
                    "provider": method.provider,
                    "is_default": method.is_default
                }),
                ip_address: None,
            })
            .await?;
        Ok(method)
    }
}

pub struct RemovePaymentMethod {
    pub repo: Arc<dyn PaymentMethodRepository>,
    pub audit: Arc<dyn AuditLogger>,
}

impl RemovePaymentMethod {
    pub async fn execute(&self, user_id: Uuid, id: Uuid) -> Result<(), PaymentError> {
        self.repo.delete(user_id, id).await?;
        self.audit
            .log(AuditLogInput {
                actor_type: "user".into(),
                actor_id: Some(user_id),
                action: "payment_method.removed".into(),
                resource_type: "payment_method".into(),
                resource_id: Some(id),
                metadata: json!({}),
                ip_address: None,
            })
            .await
    }
}

pub struct SetDefaultPaymentMethod {
    pub repo: Arc<dyn PaymentMethodRepository>,
    pub audit: Arc<dyn AuditLogger>,
}

impl SetDefaultPaymentMethod {
    pub async fn execute(&self, user_id: Uuid, id: Uuid) -> Result<PaymentMethod, PaymentError> {
        self.repo.set_default(user_id, id).await?;
        let method = self
            .repo
            .find_by_id(id)
            .await?
            .ok_or(PaymentError::PaymentMethodNotFound)?;
        self.audit
            .log(AuditLogInput {
                actor_type: "user".into(),
                actor_id: Some(user_id),
                action: "payment_method.default_changed".into(),
                resource_type: "payment_method".into(),
                resource_id: Some(id),
                metadata: json!({ "provider": method.provider }),
                ip_address: None,
            })
            .await?;
        Ok(method)
    }
}

pub struct CreatePayPalVaultSetupToken {
    pub provider: Arc<dyn PaymentProvider>,
    pub audit: Arc<dyn AuditLogger>,
}

impl CreatePayPalVaultSetupToken {
    pub async fn execute(
        &self,
        user_id: Uuid,
        return_url: String,
        cancel_url: String,
    ) -> Result<ProviderVaultSetup, PaymentError> {
        let setup = self
            .provider
            .create_vault_setup_token(user_id, return_url, cancel_url)
            .await?;
        self.audit
            .log(AuditLogInput {
                actor_type: "user".into(),
                actor_id: Some(user_id),
                action: "payment.paypal_vault_setup.created".into(),
                resource_type: "payment_method".into(),
                resource_id: None,
                metadata: json!({
                    "provider": PROVIDER_PAYPAL,
                    "setup_token_id": setup.setup_token_id,
                    "status": setup.status
                }),
                ip_address: None,
            })
            .await?;
        Ok(setup)
    }
}

pub struct CreatePayPalPaymentToken {
    pub provider: Arc<dyn PaymentProvider>,
    pub methods: Arc<dyn PaymentMethodRepository>,
    pub audit: Arc<dyn AuditLogger>,
}

impl CreatePayPalPaymentToken {
    pub async fn execute(
        &self,
        user_id: Uuid,
        setup_token_id: String,
        set_default: bool,
    ) -> Result<(PaymentMethod, ProviderPaymentToken), PaymentError> {
        let token = self
            .provider
            .create_payment_token(user_id, &setup_token_id)
            .await?;
        let method = PaymentMethod {
            id: Uuid::now_v7(),
            user_id,
            provider: PROVIDER_PAYPAL.into(),
            provider_payment_method_id: token.payment_token_id.clone(),
            brand: Some("paypal".into()),
            last4: None,
            exp_month: None,
            exp_year: None,
            is_default: set_default,
            created_at: Utc::now(),
        };
        self.methods.save(&method).await?;
        if set_default {
            self.methods.set_default(user_id, method.id).await?;
        }
        self.audit
            .log(AuditLogInput {
                actor_type: "user".into(),
                actor_id: Some(user_id),
                action: "payment.paypal_payment_token.created".into(),
                resource_type: "payment_method".into(),
                resource_id: Some(method.id),
                metadata: json!({
                    "provider": PROVIDER_PAYPAL,
                    "payer_id": token.payer_id,
                    "email": token.email,
                    "is_default": set_default
                }),
                ip_address: None,
            })
            .await?;
        Ok((method, token))
    }
}

pub struct HandlePayPalWebhook {
    pub provider: Arc<dyn PaymentProvider>,
    pub provider_events: Arc<dyn PaymentProviderEventRepository>,
    pub transactions: Arc<dyn PaymentTransactionRepository>,
    pub audit: Arc<dyn AuditLogger>,
}

impl HandlePayPalWebhook {
    pub async fn execute(
        &self,
        headers: PayPalWebhookHeaders,
        event: Value,
    ) -> Result<(), PaymentError> {
        self.provider.verify_webhook(&headers, &event).await?;
        let event_id = event
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| PaymentError::Validation("paypal webhook id missing".into()))?;
        let event_type = event
            .get("event_type")
            .and_then(Value::as_str)
            .ok_or_else(|| PaymentError::Validation("paypal webhook event_type missing".into()))?;
        let webhook_event = save_event_once(
            &self.provider_events,
            PROVIDER_PAYPAL,
            event_id.to_string(),
            event_type.to_string(),
            event.clone(),
            false,
        )
        .await?;
        let saved_webhook_event = match webhook_event {
            Some(saved) => saved,
            None => {
                let existing = self
                    .provider_events
                    .find_by_provider_event_id(PROVIDER_PAYPAL, event_id)
                    .await?
                    .ok_or(PaymentError::PaymentProviderEventNotFound)?;
                if existing.processed_at.is_some() {
                    return Ok(());
                }
                existing
            }
        };
        match event_type {
            "PAYMENT.CAPTURE.COMPLETED" => {
                self.handle_capture_completed(&event).await?;
                self.provider_events
                    .mark_processed(saved_webhook_event.id)
                    .await?;
            }
            "PAYMENT.CAPTURE.DENIED" | "PAYMENT.CAPTURE.DECLINED" => {
                self.handle_capture_failed(&event, event_type).await?;
                self.provider_events
                    .mark_processed(saved_webhook_event.id)
                    .await?;
            }
            _ => {
                self.provider_events
                    .mark_processed(saved_webhook_event.id)
                    .await?;
            }
        }
        Ok(())
    }

    async fn handle_capture_completed(&self, event: &Value) -> Result<(), PaymentError> {
        let resource = event
            .get("resource")
            .ok_or_else(|| PaymentError::Validation("paypal resource missing".into()))?;
        let order_id = resource
            .pointer("/supplementary_data/related_ids/order_id")
            .and_then(Value::as_str)
            .ok_or_else(|| PaymentError::Validation("paypal order id missing".into()))?;
        let order_event = self
            .provider_events
            .find_by_provider_event_id(PROVIDER_PAYPAL, order_id)
            .await?
            .ok_or(PaymentError::PaymentProviderEventNotFound)?;
        let context = top_up_context(&order_event.payload)?;
        let capture = CapturedPayment {
            provider_reference: resource
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            order_id: order_id.to_string(),
            amount: resource
                .pointer("/amount/value")
                .and_then(Value::as_str)
                .ok_or_else(|| PaymentError::Validation("paypal amount missing".into()))?
                .parse()
                .map_err(|e| PaymentError::Validation(format!("invalid paypal amount: {e}")))?,
            currency: resource
                .pointer("/amount/currency_code")
                .and_then(Value::as_str)
                .unwrap_or(&context.currency)
                .to_string(),
            status: "COMPLETED".into(),
        };
        let tolerance = Decimal::new(1, 2);
        if (capture.amount - context.amount).abs() > tolerance {
            return Err(PaymentError::Validation(format!(
                "webhook amount {} does not match order amount {}",
                capture.amount, context.amount
            )));
        }
        publish_success_once(
            &self.provider_events,
            &self.transactions,
            &context,
            &capture,
        )
        .await?;
        self.audit
            .log(AuditLogInput {
                actor_type: "system".into(),
                actor_id: None,
                action: "payment.paypal_webhook.capture_completed".into(),
                resource_type: "payment_transaction".into(),
                resource_id: Some(context.transaction_id),
                metadata: json!({
                    "order_id": order_id,
                    "capture_id": capture.provider_reference
                }),
                ip_address: None,
            })
            .await
    }

    async fn handle_capture_failed(
        &self,
        event: &Value,
        event_type: &str,
    ) -> Result<(), PaymentError> {
        let resource = event
            .get("resource")
            .ok_or_else(|| PaymentError::Validation("paypal resource missing".into()))?;
        let order_id = resource
            .pointer("/supplementary_data/related_ids/order_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let Some(order_event) = self
            .provider_events
            .find_by_provider_event_id(PROVIDER_PAYPAL, order_id)
            .await?
        else {
            return Ok(());
        };
        let context = top_up_context(&order_event.payload)?;
        if let Some(mut transaction) = self.transactions.find_by_id(context.transaction_id).await? {
            transaction.status = "failed".into();
            transaction.failure_reason = Some(event_type.to_string());
            transaction.updated_at = Utc::now();
            self.transactions.update(&transaction).await?;
        }
        self.audit
            .log(AuditLogInput {
                actor_type: "system".into(),
                actor_id: None,
                action: "payment.paypal_webhook.capture_failed".into(),
                resource_type: "payment_transaction".into(),
                resource_id: Some(context.transaction_id),
                metadata: json!({ "event_type": event_type }),
                ip_address: None,
            })
            .await
    }
}
