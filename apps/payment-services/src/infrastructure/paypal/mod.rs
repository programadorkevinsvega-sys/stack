use crate::application::ports::{CreateOrderInput, PayPalWebhookHeaders, PaymentProvider};
use crate::domain::entities::{
    CapturedPayment, ProviderOrder, ProviderPaymentToken, ProviderVaultSetup,
};
use crate::domain::errors::PaymentError;
use async_trait::async_trait;
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::time::sleep;

pub struct PayPalClient {
    http: Client,
    base_url: String,
    client_id: String,
    client_secret: String,
    webhook_id: String,
    token_cache: Mutex<Option<(String, Instant)>>,
}

impl PayPalClient {
    pub fn new(
        base_url: String,
        client_id: String,
        client_secret: String,
        webhook_id: String,
    ) -> Result<Self, PaymentError> {
        if base_url.trim().is_empty()
            || client_id.trim().is_empty()
            || client_secret.trim().is_empty()
            || webhook_id.trim().is_empty()
        {
            return Err(PaymentError::Validation(
                "paypal configuration cannot be empty".into(),
            ));
        }
        Ok(Self {
            http: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            client_id,
            client_secret,
            webhook_id,
            token_cache: Mutex::new(None),
        })
    }

    async fn access_token(&self) -> Result<String, PaymentError> {
        {
            let cache = self.token_cache.lock().unwrap();
            if let Some((ref token, expires)) = *cache {
                if Instant::now() + Duration::from_secs(300) < expires {
                    return Ok(token.clone());
                }
            }
        }
        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: String,
            #[serde(default)]
            expires_in: u64,
        }

        let response = self
            .send_with_retry(
                self.http
                    .post(format!("{}/v1/oauth2/token", self.base_url))
                    .basic_auth(&self.client_id, Some(&self.client_secret))
                    .form(&[("grant_type", "client_credentials")]),
                "paypal oauth",
            )
            .await?;
        if !response.status().is_success() {
            return Err(PaymentError::ProviderError(format!(
                "paypal oauth failed with status {}",
                response.status()
            )));
        }
        let body = response
            .json::<TokenResponse>()
            .await
            .map_err(|e| PaymentError::ProviderError(e.to_string()))?;
        let expires_in = if body.expires_in > 0 {
            body.expires_in
        } else {
            3600
        };
        let expires = Instant::now() + Duration::from_secs(expires_in);
        {
            let mut cache = self.token_cache.lock().unwrap();
            *cache = Some((body.access_token.clone(), expires));
        }
        Ok(body.access_token)
    }

    async fn send_with_retry(
        &self,
        request: RequestBuilder,
        operation: &str,
    ) -> Result<Response, PaymentError> {
        let mut delay = Duration::from_millis(150);
        let mut last_status = None;
        for attempt in 0..3 {
            let Some(cloned) = request.try_clone() else {
                return Err(PaymentError::ProviderError(format!(
                    "{operation} request cannot be retried"
                )));
            };
            match cloned.send().await {
                Ok(response) if !is_retryable(response.status()) => return Ok(response),
                Ok(response) => {
                    last_status = Some(response.status());
                    if attempt == 2 {
                        return Ok(response);
                    }
                }
                Err(error) => {
                    if attempt == 2 {
                        return Err(PaymentError::ProviderError(format!(
                            "{operation} failed after retries: {error}"
                        )));
                    }
                }
            }
            sleep(delay).await;
            delay *= 2;
        }
        Err(PaymentError::ProviderError(format!(
            "{operation} failed after retries: {:?}",
            last_status
        )))
    }
}

fn is_retryable(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn decimal_amount(value: Decimal) -> String {
    format!("{:.2}", value.round_dp(2))
}

fn first_approval_url(links: &[PayPalLink]) -> Result<String, PaymentError> {
    links
        .iter()
        .find(|link| link.rel == "approve")
        .map(|link| link.href.clone())
        .ok_or_else(|| PaymentError::ProviderError("paypal approval link missing".into()))
}

#[derive(Deserialize)]
struct PayPalLink {
    href: String,
    rel: String,
}

#[derive(Deserialize)]
struct CreateOrderResponse {
    id: String,
    status: String,
    links: Vec<PayPalLink>,
}

#[derive(Deserialize)]
struct CreateSetupTokenResponse {
    id: String,
    status: String,
    links: Vec<PayPalLink>,
}

#[derive(Deserialize)]
struct CreatePaymentTokenResponse {
    id: String,
    payment_source: Option<PaymentTokenSource>,
}

#[derive(Deserialize)]
struct PaymentTokenSource {
    paypal: Option<PaymentTokenPayPal>,
}

#[derive(Deserialize)]
struct PaymentTokenPayPal {
    payer_id: Option<String>,
    email_address: Option<String>,
}

#[derive(Deserialize)]
struct CaptureOrderResponse {
    id: String,
    status: String,
    purchase_units: Vec<CapturedPurchaseUnit>,
}

#[derive(Deserialize)]
struct CapturedPurchaseUnit {
    payments: Option<CapturedPayments>,
}

#[derive(Deserialize)]
struct CapturedPayments {
    captures: Vec<PayPalCapture>,
}

#[derive(Deserialize)]
struct PayPalCapture {
    id: String,
    status: String,
    amount: PayPalAmount,
}

#[derive(Deserialize)]
struct PayPalAmount {
    value: String,
    currency_code: String,
}

#[derive(Serialize)]
struct VerifyWebhookRequest<'a> {
    webhook_id: &'a str,
    transmission_id: &'a str,
    transmission_time: &'a str,
    cert_url: &'a str,
    auth_algo: &'a str,
    transmission_sig: &'a str,
    webhook_event: &'a Value,
}

#[derive(Deserialize)]
struct VerifyWebhookResponse {
    verification_status: String,
}

#[async_trait]
impl PaymentProvider for PayPalClient {
    async fn create_order(&self, input: CreateOrderInput) -> Result<ProviderOrder, PaymentError> {
        if input.amount <= Decimal::ZERO {
            return Err(PaymentError::Validation("amount must be positive".into()));
        }
        let currency = input.currency.trim().to_uppercase();
        if currency.len() != 3 {
            return Err(PaymentError::Validation("currency must be ISO-4217".into()));
        }
        let token = self.access_token().await?;
        let response = self
            .send_with_retry(
                self.http
                    .post(format!("{}/v2/checkout/orders", self.base_url))
                    .bearer_auth(token)
                    .header("PayPal-Request-Id", &input.idempotency_key)
                    .json(&json!({
                        "intent": "CAPTURE",
                        "purchase_units": [{
                            "reference_id": input.idempotency_key,
                            "custom_id": input.user_id.to_string(),
                            "amount": {
                                "currency_code": currency,
                                "value": decimal_amount(input.amount)
                            }
                        }],
                        "application_context": {
                            "return_url": input.return_url,
                            "cancel_url": input.cancel_url,
                            "user_action": "PAY_NOW"
                        }
                    })),
                "paypal create order",
            )
            .await?;
        if !response.status().is_success() {
            return Err(PaymentError::ProviderError(format!(
                "paypal create order failed with status {}",
                response.status()
            )));
        }
        let body = response
            .json::<CreateOrderResponse>()
            .await
            .map_err(|e| PaymentError::ProviderError(e.to_string()))?;
        Ok(ProviderOrder {
            approval_url: first_approval_url(&body.links)?,
            provider_reference: body.id,
            status: body.status,
        })
    }

    async fn capture_order(&self, order_id: &str) -> Result<CapturedPayment, PaymentError> {
        if order_id.trim().is_empty() {
            return Err(PaymentError::Validation("order_id is required".into()));
        }
        let token = self.access_token().await?;
        let response = self
            .send_with_retry(
                self.http
                    .post(format!(
                        "{}/v2/checkout/orders/{}/capture",
                        self.base_url, order_id
                    ))
                    .bearer_auth(token),
                "paypal capture order",
            )
            .await?;
        if !response.status().is_success() {
            return Err(PaymentError::ProviderError(format!(
                "paypal capture order failed with status {}",
                response.status()
            )));
        }
        let body = response
            .json::<CaptureOrderResponse>()
            .await
            .map_err(|e| PaymentError::ProviderError(e.to_string()))?;
        let capture = body
            .purchase_units
            .into_iter()
            .find_map(|unit| unit.payments)
            .and_then(|payments| payments.captures.into_iter().next())
            .ok_or_else(|| PaymentError::ProviderError("paypal capture missing".into()))?;
        if body.status != "COMPLETED" || capture.status != "COMPLETED" {
            return Err(PaymentError::PaymentFailed(format!(
                "paypal order status={}, capture status={}",
                body.status, capture.status
            )));
        }
        Ok(CapturedPayment {
            provider_reference: capture.id,
            order_id: body.id,
            amount: capture
                .amount
                .value
                .parse()
                .map_err(|e| PaymentError::ProviderError(format!("invalid paypal amount: {e}")))?,
            currency: capture.amount.currency_code,
            status: capture.status,
        })
    }

    async fn charge_vault_token(
        &self,
        user_id: uuid::Uuid,
        payment_token_id: &str,
        amount: Decimal,
        currency: &str,
        idempotency_key: &str,
    ) -> Result<CapturedPayment, PaymentError> {
        if payment_token_id.trim().is_empty() || amount <= Decimal::ZERO {
            return Err(PaymentError::Validation(
                "valid vault token and amount are required".into(),
            ));
        }
        let token = self.access_token().await?;
        let response = self.send_with_retry(self.http.post(format!("{}/v2/checkout/orders", self.base_url)).bearer_auth(token).header("PayPal-Request-Id", idempotency_key).json(&json!({"intent":"CAPTURE","purchase_units":[{"reference_id":idempotency_key,"custom_id":user_id.to_string(),"amount":{"currency_code":currency.to_uppercase(),"value":decimal_amount(amount)}}],"payment_source":{"paypal":{"attributes":{"vault":{"id":payment_token_id,"stored_payment_source":{"payment_initiated_type":"MERCHANT","payment_type":"RECURRING","usage_type":"MERCHANT"}}}}}})), "paypal vault order").await?;
        if !response.status().is_success() {
            return Err(PaymentError::ProviderError(format!(
                "paypal vault order failed with status {}",
                response.status()
            )));
        }
        let body = response
            .json::<CreateOrderResponse>()
            .await
            .map_err(|e| PaymentError::ProviderError(e.to_string()))?;
        self.capture_order(&body.id).await
    }

    async fn create_vault_setup_token(
        &self,
        user_id: uuid::Uuid,
        return_url: String,
        cancel_url: String,
    ) -> Result<ProviderVaultSetup, PaymentError> {
        let token = self.access_token().await?;
        let response = self
            .send_with_retry(
                self.http
                    .post(format!("{}/v3/vault/setup-tokens", self.base_url))
                    .bearer_auth(token)
                    .json(&json!({
                        "payment_source": {
                            "paypal": {
                                "usage_type": "MERCHANT",
                                "experience_context": {
                                    "return_url": return_url,
                                    "cancel_url": cancel_url,
                                    "shipping_preference": "NO_SHIPPING",
                                    "user_action": "CONTINUE"
                                }
                            }
                        },
                        "customer": {
                            "merchant_customer_id": user_id.to_string()
                        }
                    })),
                "paypal create vault setup token",
            )
            .await?;
        if !response.status().is_success() {
            return Err(PaymentError::ProviderError(format!(
                "paypal create setup token failed with status {}",
                response.status()
            )));
        }
        let body = response
            .json::<CreateSetupTokenResponse>()
            .await
            .map_err(|e| PaymentError::ProviderError(e.to_string()))?;
        Ok(ProviderVaultSetup {
            approval_url: first_approval_url(&body.links)?,
            setup_token_id: body.id,
            status: body.status,
        })
    }

    async fn create_payment_token(
        &self,
        user_id: uuid::Uuid,
        setup_token_id: &str,
    ) -> Result<ProviderPaymentToken, PaymentError> {
        if setup_token_id.trim().is_empty() {
            return Err(PaymentError::Validation(
                "setup_token_id is required".into(),
            ));
        }
        let token = self.access_token().await?;
        let response = self
            .send_with_retry(
                self.http
                    .post(format!("{}/v3/vault/payment-tokens", self.base_url))
                    .bearer_auth(token)
                    .json(&json!({
                        "payment_source": {
                            "token": {
                                "id": setup_token_id,
                                "type": "SETUP_TOKEN"
                            }
                        },
                        "customer": {
                            "merchant_customer_id": user_id.to_string()
                        }
                    })),
                "paypal create payment token",
            )
            .await?;
        if !response.status().is_success() {
            return Err(PaymentError::ProviderError(format!(
                "paypal create payment token failed with status {}",
                response.status()
            )));
        }
        let body = response
            .json::<CreatePaymentTokenResponse>()
            .await
            .map_err(|e| PaymentError::ProviderError(e.to_string()))?;
        let paypal = body.payment_source.and_then(|source| source.paypal);
        Ok(ProviderPaymentToken {
            payment_token_id: body.id,
            payer_id: paypal.as_ref().and_then(|value| value.payer_id.clone()),
            email: paypal.and_then(|value| value.email_address),
        })
    }

    async fn verify_webhook(
        &self,
        headers: &PayPalWebhookHeaders,
        event: &Value,
    ) -> Result<(), PaymentError> {
        let token = self.access_token().await?;
        let response = self
            .send_with_retry(
                self.http
                    .post(format!(
                        "{}/v1/notifications/verify-webhook-signature",
                        self.base_url
                    ))
                    .bearer_auth(token)
                    .json(&VerifyWebhookRequest {
                        webhook_id: &self.webhook_id,
                        transmission_id: &headers.transmission_id,
                        transmission_time: &headers.transmission_time,
                        cert_url: &headers.cert_url,
                        auth_algo: &headers.auth_algo,
                        transmission_sig: &headers.transmission_sig,
                        webhook_event: event,
                    }),
                "paypal verify webhook",
            )
            .await?;
        if !response.status().is_success() {
            return Err(PaymentError::InvalidWebhookSignature);
        }
        let body = response
            .json::<VerifyWebhookResponse>()
            .await
            .map_err(|e| PaymentError::ProviderError(e.to_string()))?;
        if body.verification_status == "SUCCESS" {
            Ok(())
        } else {
            Err(PaymentError::InvalidWebhookSignature)
        }
    }
}
