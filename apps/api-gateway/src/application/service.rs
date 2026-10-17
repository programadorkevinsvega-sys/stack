use crate::domain::errors::GatewayError;
use crate::infrastructure::clients::{
    BillingClient, CatalogClient, CredentialClient, ModelRouterClient, PlanClient,
};
use crate::infrastructure::events::GatewayEventPublisher;
use crate::infrastructure::key_cache::CachedKeyClient;
use crate::infrastructure::rate_limit::{RateLimitInput, RedisRateLimiter};
use base64::Engine;
use bedrock_client::{
    ChatCompletionRequest as BedrockChatRequest, ChatCompletionResponse as BedrockChatResponse,
    EmbeddingInput, EmbeddingRequest as BedrockEmbeddingRequest,
    EmbeddingResponse as BedrockEmbeddingResponse, MantleCredentials, TokenUsage,
};
use chrono::Utc;
use common::RequestId;
use events::{RequestFailed, UsageRecorded};
use futures_util::{Stream, StreamExt};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct GatewayConfig {
    pub mantle_base_url: String,
    pub reserve_safety_multiplier: Decimal,
}

#[derive(Clone)]
pub struct ApiGatewayApplication {
    pub keys: Arc<CachedKeyClient>,
    pub plans: Arc<PlanClient>,
    pub catalog: Arc<CatalogClient>,
    pub billing: Arc<BillingClient>,
    pub router: Arc<ModelRouterClient>,
    pub credentials: Arc<CredentialClient>,
    pub limiter: Arc<RedisRateLimiter>,
    pub events: Arc<GatewayEventPublisher>,
    pub bedrock: bedrock_client::Client,
    pub config: GatewayConfig,
}

#[derive(Debug, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<bedrock_client::ChatMessage>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    #[serde(default)]
    pub stream: bool,
    pub tools: Option<Vec<bedrock_client::ToolDef>>,
    pub user: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChatChoice>,
    pub usage: ChatUsage,
}

#[derive(Debug, Serialize)]
pub struct ChatChoice {
    pub index: i32,
    pub message: ChatMessageResponse,
    pub finish_reason: String,
}

#[derive(Debug, Serialize)]
pub struct ChatMessageResponse {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct ChatUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Deserialize)]
pub struct EmbeddingRequest {
    pub model: String,
    pub input: EmbeddingInput,
    pub encoding_format: Option<String>,
    pub dimensions: Option<u32>,
    pub user: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EmbeddingResponse {
    pub object: String,
    pub data: Vec<bedrock_client::EmbeddingData>,
    pub model: String,
    pub usage: EmbeddingUsage,
}

#[derive(Debug, Serialize)]
pub struct EmbeddingUsage {
    pub prompt_tokens: u32,
    pub total_tokens: u32,
}

pub type ChatCompletionStream =
    Pin<Box<dyn Stream<Item = Result<String, GatewayError>> + Send + 'static>>;

#[derive(Clone)]
pub struct GatewayIdentity {
    pub api_key_id: Uuid,
    pub user_id: Uuid,
}

struct UsagePublishInput {
    request_id: Uuid,
    model_catalog_id: Uuid,
    aws_account_id: Uuid,
    input_tokens: u32,
    output_tokens: u32,
    stream: bool,
    latency_ms: i32,
    status_code: i32,
    error_code: Option<String>,
}

impl ApiGatewayApplication {
    async fn resolve_mantle_credentials(
        &self,
        account_id: Uuid,
        model_catalog_id: Uuid,
    ) -> Result<MantleCredentials, GatewayError> {
        if let Some(connection) = self
            .credentials
            .get_mantle_connection(account_id, model_catalog_id)
            .await?
        {
            return Ok(MantleCredentials {
                api_key: connection.api_key,
                base_url: connection.base_url,
            });
        }
        let aws_credentials = self.credentials.get_credentials(account_id).await?;
        Ok(MantleCredentials {
            api_key: encode_mantle_api_key(&aws_credentials),
            base_url: self.config.mantle_base_url.clone(),
        })
    }

    pub async fn proxy_chat_completion(
        &self,
        api_key: &str,
        source_ip: Option<String>,
        request: ChatCompletionRequest,
        request_id: RequestId,
    ) -> Result<ChatCompletionResponse, GatewayError> {
        if request.stream {
            return Err(GatewayError::Validation(
                "stream=true will be handled by the streaming endpoint path".into(),
            ));
        }
        validate_chat_request(&request)?;
        let request_uuid = Uuid::new_v4();
        let started = std::time::Instant::now();
        let key = self.keys.validate(api_key, source_ip).await?;
        let identity = GatewayIdentity {
            api_key_id: key.api_key_id,
            user_id: key.user_id,
        };
        let plan = self.plans.get_plan_limits(identity.user_id).await?;
        if !plan.has_active_subscription {
            return Err(GatewayError::PaymentRequired);
        }
        let model = self
            .catalog
            .get_model_by_public_name(&request.model, Some(&plan.plan_id))
            .await?;
        let estimated_tokens = estimate_tokens(&request);
        self.limiter
            .check(RateLimitInput {
                api_key_id: identity.api_key_id,
                rpm_limit: key
                    .rate_limit_override_rpm
                    .unwrap_or(plan.requests_per_minute)
                    .max(1),
                tpm_limit: key
                    .rate_limit_override_tpm
                    .unwrap_or(plan.tokens_per_minute)
                    .max(1),
                concurrent_limit: key
                    .max_concurrent_requests
                    .unwrap_or(plan.max_concurrent_requests)
                    .max(1),
                estimated_tokens,
            })
            .await?;
        let reservation = self
            .billing
            .reserve(
                identity.user_id,
                request_uuid,
                estimated_tokens,
                estimate_reserve_amount(
                    estimated_tokens,
                    &model.price_per_1k_input,
                    &model.price_per_1k_output,
                    self.config.reserve_safety_multiplier,
                )?,
            )
            .await?;
        if !reservation.approved {
            self.limiter.finish(identity.api_key_id).await;
            return Err(GatewayError::PaymentRequired);
        }

        let selected = match self
            .router
            .select_account(model.model_catalog_id, request_uuid, estimated_tokens)
            .await
        {
            Ok(selected) => selected,
            Err(error) => {
                self.billing
                    .release(identity.user_id, reservation.reservation_id)
                    .await;
                self.limiter.finish(identity.api_key_id).await;
                return Err(error);
            }
        };
        let mantle = self
            .resolve_mantle_credentials(selected.account_id, model.model_catalog_id)
            .await?;
        let bedrock_request = BedrockChatRequest {
            model: selected.provider_model_id.clone(),
            messages: request.messages.clone(),
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            stream: false,
            tools: request.tools.clone(),
        };
        let response = match self.bedrock.converse(&mantle, &bedrock_request).await {
            Ok(response) => response,
            Err(error) => {
                let latency_ms = started.elapsed().as_millis() as i32;
                self.billing
                    .release(identity.user_id, reservation.reservation_id)
                    .await;
                self.router
                    .report_outcome(
                        selected.account_id,
                        request_uuid,
                        false,
                        Some(error.to_string()),
                        0,
                        0,
                    )
                    .await;
                self.publish_failure(
                    &identity,
                    request_uuid,
                    selected.account_id,
                    &error.to_string(),
                    request_id,
                )
                .await;
                self.publish_usage(
                    &identity,
                    UsagePublishInput {
                        request_id: request_uuid,
                        model_catalog_id: model.model_catalog_id,
                        aws_account_id: selected.account_id,
                        input_tokens: 0,
                        output_tokens: 0,
                        stream: false,
                        latency_ms,
                        status_code: 502,
                        error_code: Some("bedrock_error".into()),
                    },
                    request_id,
                )
                .await;
                self.limiter.finish(identity.api_key_id).await;
                return Err(GatewayError::UpstreamUnavailable(error.to_string()));
            }
        };
        self.billing
            .confirm(
                identity.user_id,
                reservation.reservation_id,
                request_uuid,
                model.model_catalog_id,
                response.usage.input_tokens,
                response.usage.output_tokens,
            )
            .await?;
        self.router
            .report_outcome(
                selected.account_id,
                request_uuid,
                true,
                None,
                i64::from(response.usage.input_tokens),
                i64::from(response.usage.output_tokens),
            )
            .await;
        self.publish_usage(
            &identity,
            UsagePublishInput {
                request_id: request_uuid,
                model_catalog_id: model.model_catalog_id,
                aws_account_id: selected.account_id,
                input_tokens: response.usage.input_tokens,
                output_tokens: response.usage.output_tokens,
                stream: false,
                latency_ms: response.latency_ms as i32,
                status_code: 200,
                error_code: None,
            },
            request_id,
        )
        .await;
        self.limiter.finish(identity.api_key_id).await;
        Ok(openai_response(&request.model, response))
    }

    pub async fn proxy_embeddings(
        &self,
        api_key: &str,
        source_ip: Option<String>,
        request: EmbeddingRequest,
        request_id: RequestId,
    ) -> Result<EmbeddingResponse, GatewayError> {
        validate_embedding_request(&request)?;
        let request_uuid = Uuid::now_v7();
        let started = std::time::Instant::now();
        let key = self.keys.validate(api_key, source_ip).await?;
        let identity = GatewayIdentity {
            api_key_id: key.api_key_id,
            user_id: key.user_id,
        };
        let plan = self.plans.get_plan_limits(identity.user_id).await?;
        if !plan.has_active_subscription {
            return Err(GatewayError::PaymentRequired);
        }
        let model = self
            .catalog
            .get_model_by_public_name(&request.model, Some(&plan.plan_id))
            .await?;
        let estimated_tokens = estimate_embedding_tokens(&request);
        self.limiter
            .check(RateLimitInput {
                api_key_id: identity.api_key_id,
                rpm_limit: key
                    .rate_limit_override_rpm
                    .unwrap_or(plan.requests_per_minute)
                    .max(1),
                tpm_limit: key
                    .rate_limit_override_tpm
                    .unwrap_or(plan.tokens_per_minute)
                    .max(1),
                concurrent_limit: key
                    .max_concurrent_requests
                    .unwrap_or(plan.max_concurrent_requests)
                    .max(1),
                estimated_tokens,
            })
            .await?;
        let reservation = self
            .billing
            .reserve(
                identity.user_id,
                request_uuid,
                estimated_tokens,
                estimate_embedding_reserve_amount(
                    estimated_tokens,
                    &model.price_per_1k_input,
                    self.config.reserve_safety_multiplier,
                ),
            )
            .await?;
        if !reservation.approved {
            self.limiter.finish(identity.api_key_id).await;
            return Err(GatewayError::PaymentRequired);
        }
        let selected = match self
            .router
            .select_account(model.model_catalog_id, request_uuid, estimated_tokens)
            .await
        {
            Ok(selected) => selected,
            Err(error) => {
                self.billing
                    .release(identity.user_id, reservation.reservation_id)
                    .await;
                self.limiter.finish(identity.api_key_id).await;
                return Err(error);
            }
        };
        let mantle = self
            .resolve_mantle_credentials(selected.account_id, model.model_catalog_id)
            .await?;
        let bedrock_request = BedrockEmbeddingRequest {
            model: selected.provider_model_id.clone(),
            input: request.input.clone(),
            encoding_format: request.encoding_format.clone(),
            dimensions: request.dimensions,
            user: request.user.clone(),
        };
        let response = match self.bedrock.embeddings(&mantle, &bedrock_request).await {
            Ok(response) => response,
            Err(error) => {
                let latency_ms = started.elapsed().as_millis() as i32;
                self.billing
                    .release(identity.user_id, reservation.reservation_id)
                    .await;
                self.router
                    .report_outcome(
                        selected.account_id,
                        request_uuid,
                        false,
                        Some(error.to_string()),
                        0,
                        0,
                    )
                    .await;
                self.publish_failure(
                    &identity,
                    request_uuid,
                    selected.account_id,
                    &error.to_string(),
                    request_id,
                )
                .await;
                self.publish_usage(
                    &identity,
                    UsagePublishInput {
                        request_id: request_uuid,
                        model_catalog_id: model.model_catalog_id,
                        aws_account_id: selected.account_id,
                        input_tokens: 0,
                        output_tokens: 0,
                        stream: false,
                        latency_ms,
                        status_code: 502,
                        error_code: Some("bedrock_embeddings_error".into()),
                    },
                    request_id,
                )
                .await;
                self.limiter.finish(identity.api_key_id).await;
                return Err(GatewayError::UpstreamUnavailable(error.to_string()));
            }
        };
        self.billing
            .confirm(
                identity.user_id,
                reservation.reservation_id,
                request_uuid,
                model.model_catalog_id,
                response.usage.input_tokens,
                0,
            )
            .await?;
        self.router
            .report_outcome(
                selected.account_id,
                request_uuid,
                true,
                None,
                i64::from(response.usage.input_tokens),
                0,
            )
            .await;
        self.publish_usage(
            &identity,
            UsagePublishInput {
                request_id: request_uuid,
                model_catalog_id: model.model_catalog_id,
                aws_account_id: selected.account_id,
                input_tokens: response.usage.input_tokens,
                output_tokens: 0,
                stream: false,
                latency_ms: response.latency_ms as i32,
                status_code: 200,
                error_code: None,
            },
            request_id,
        )
        .await;
        self.limiter.finish(identity.api_key_id).await;
        Ok(openai_embedding_response(&request.model, response))
    }

    pub async fn proxy_chat_completion_stream(
        &self,
        api_key: &str,
        source_ip: Option<String>,
        request: ChatCompletionRequest,
        request_id: RequestId,
    ) -> Result<ChatCompletionStream, GatewayError> {
        validate_chat_request(&request)?;
        let request_uuid = Uuid::new_v4();
        let started = std::time::Instant::now();
        let key = self.keys.validate(api_key, source_ip).await?;
        let identity = GatewayIdentity {
            api_key_id: key.api_key_id,
            user_id: key.user_id,
        };
        let plan = self.plans.get_plan_limits(identity.user_id).await?;
        if !plan.has_active_subscription {
            return Err(GatewayError::PaymentRequired);
        }
        let model = self
            .catalog
            .get_model_by_public_name(&request.model, Some(&plan.plan_id))
            .await?;
        let estimated_tokens = estimate_tokens(&request);
        self.limiter
            .check(RateLimitInput {
                api_key_id: identity.api_key_id,
                rpm_limit: key
                    .rate_limit_override_rpm
                    .unwrap_or(plan.requests_per_minute)
                    .max(1),
                tpm_limit: key
                    .rate_limit_override_tpm
                    .unwrap_or(plan.tokens_per_minute)
                    .max(1),
                concurrent_limit: key
                    .max_concurrent_requests
                    .unwrap_or(plan.max_concurrent_requests)
                    .max(1),
                estimated_tokens,
            })
            .await?;
        let reservation = self
            .billing
            .reserve(
                identity.user_id,
                request_uuid,
                estimated_tokens,
                estimate_reserve_amount(
                    estimated_tokens,
                    &model.price_per_1k_input,
                    &model.price_per_1k_output,
                    self.config.reserve_safety_multiplier,
                )?,
            )
            .await?;
        if !reservation.approved {
            self.limiter.finish(identity.api_key_id).await;
            return Err(GatewayError::PaymentRequired);
        }
        let selected = match self
            .router
            .select_account(model.model_catalog_id, request_uuid, estimated_tokens)
            .await
        {
            Ok(selected) => selected,
            Err(error) => {
                self.billing
                    .release(identity.user_id, reservation.reservation_id)
                    .await;
                self.limiter.finish(identity.api_key_id).await;
                return Err(error);
            }
        };
        let mantle = self
            .resolve_mantle_credentials(selected.account_id, model.model_catalog_id)
            .await?;
        let bedrock_request = BedrockChatRequest {
            model: selected.provider_model_id.clone(),
            messages: request.messages.clone(),
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            stream: true,
            tools: request.tools.clone(),
        };
        let upstream = match self.bedrock.converse_stream(&mantle, bedrock_request).await {
            Ok(stream) => stream,
            Err(error) => {
                self.billing
                    .release(identity.user_id, reservation.reservation_id)
                    .await;
                self.router
                    .report_outcome(
                        selected.account_id,
                        request_uuid,
                        false,
                        Some(error.to_string()),
                        0,
                        0,
                    )
                    .await;
                self.publish_failure(
                    &identity,
                    request_uuid,
                    selected.account_id,
                    &error.to_string(),
                    request_id,
                )
                .await;
                self.limiter.finish(identity.api_key_id).await;
                return Err(GatewayError::UpstreamUnavailable(error.to_string()));
            }
        };
        let app = self.clone();
        let public_model = request.model.clone();
        let stream = async_stream::stream! {
            let mut upstream = Box::pin(upstream);
            let mut final_usage: Option<TokenUsage> = None;
            let mut output_chars = 0usize;
            while let Some(chunk) = upstream.next().await {
                match chunk {
                    Ok(chunk) => {
                        output_chars += chunk.delta.len();
                        if let Some(usage) = chunk.usage {
                            final_usage = Some(usage);
                        }
                        let payload = serde_json::json!({
                            "id": format!("chatcmpl-{}", request_uuid),
                            "object": "chat.completion.chunk",
                            "created": Utc::now().timestamp(),
                            "model": public_model,
                            "choices": [{
                                "index": 0,
                                "delta": {"content": chunk.delta},
                                "finish_reason": chunk.finish_reason
                            }]
                        });
                        yield Ok(payload.to_string());
                    }
                    Err(error) => {
                        let latency_ms = started.elapsed().as_millis() as i32;
                        app.billing.release(identity.user_id, reservation.reservation_id).await;
                        app.router.report_outcome(
                            selected.account_id,
                            request_uuid,
                            false,
                            Some(error.to_string()),
                            0,
                            0,
                        ).await;
                        app.publish_failure(&identity, request_uuid, selected.account_id, &error.to_string(), request_id).await;
                        app.publish_usage(&identity, UsagePublishInput {
                            request_id: request_uuid,
                            model_catalog_id: model.model_catalog_id,
                            aws_account_id: selected.account_id,
                            input_tokens: 0,
                            output_tokens: 0,
                            stream: true,
                            latency_ms,
                            status_code: 502,
                            error_code: Some("bedrock_stream_error".into()),
                        }, request_id).await;
                        app.limiter.finish(identity.api_key_id).await;
                        yield Err(GatewayError::UpstreamUnavailable(error.to_string()));
                        return;
                    }
                }
            }
            let usage = final_usage.unwrap_or_else(|| TokenUsage {
                input_tokens: (estimated_tokens.max(1) as u32).min(u32::MAX / 2),
                output_tokens: ((output_chars / 4).max(1) as u32).min(u32::MAX / 2),
                cached_tokens: 0,
            });
            if let Err(error) = app.billing.confirm(
                identity.user_id,
                reservation.reservation_id,
                request_uuid,
                model.model_catalog_id,
                usage.input_tokens,
                usage.output_tokens,
            ).await {
                app.router.report_outcome(
                    selected.account_id,
                    request_uuid,
                    false,
                    Some(error.to_string()),
                    i64::from(usage.input_tokens),
                    i64::from(usage.output_tokens),
                ).await;
                app.limiter.finish(identity.api_key_id).await;
                yield Err(error);
                return;
            }
            app.router.report_outcome(
                selected.account_id,
                request_uuid,
                true,
                None,
                i64::from(usage.input_tokens),
                i64::from(usage.output_tokens),
            ).await;
            app.publish_usage(&identity, UsagePublishInput {
                request_id: request_uuid,
                model_catalog_id: model.model_catalog_id,
                aws_account_id: selected.account_id,
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                stream: true,
                latency_ms: started.elapsed().as_millis() as i32,
                status_code: 200,
                error_code: None,
            }, request_id).await;
            app.limiter.finish(identity.api_key_id).await;
            yield Ok("[DONE]".into());
        };
        Ok(Box::pin(stream))
    }

    async fn publish_usage(
        &self,
        identity: &GatewayIdentity,
        input: UsagePublishInput,
        request_id: RequestId,
    ) {
        if let Err(error) = self
            .events
            .publish_usage_recorded(
                &UsageRecorded {
                    request_id: input.request_id,
                    api_key_id: identity.api_key_id,
                    user_id: identity.user_id,
                    model_catalog_id: input.model_catalog_id,
                    aws_account_id: input.aws_account_id,
                    input_tokens: input.input_tokens,
                    output_tokens: input.output_tokens,
                    cached_tokens: 0,
                    stream: input.stream,
                    latency_ms: input.latency_ms.max(0) as u32,
                    ttft_ms: None,
                    status_code: input.status_code,
                    error_code: input.error_code,
                },
                request_id,
            )
            .await
        {
            tracing::error!(
                %error,
                request_id = %input.request_id,
                "failed to enqueue usage event in outbox"
            );
        }
    }

    async fn publish_failure(
        &self,
        identity: &GatewayIdentity,
        request_id: Uuid,
        aws_account_id: Uuid,
        error: &str,
        http_request_id: RequestId,
    ) {
        if let Err(outbox_error) = self
            .events
            .publish_request_failed(
                &RequestFailed {
                    request_id,
                    api_key_id: identity.api_key_id,
                    user_id: identity.user_id,
                    error_code: "bedrock_error".into(),
                    error_message: error.into(),
                    aws_account_id: Some(aws_account_id),
                },
                http_request_id,
            )
            .await
        {
            tracing::error!(
                error = %outbox_error,
                %request_id,
                "failed to enqueue request failure event in outbox"
            );
        }
    }
}

fn validate_chat_request(request: &ChatCompletionRequest) -> Result<(), GatewayError> {
    if request.model.trim().is_empty() {
        return Err(GatewayError::InvalidRequestBody("model is required".into()));
    }
    if request.messages.is_empty() {
        return Err(GatewayError::InvalidRequestBody(
            "messages cannot be empty".into(),
        ));
    }
    Ok(())
}

fn validate_embedding_request(request: &EmbeddingRequest) -> Result<(), GatewayError> {
    if request.model.trim().is_empty() {
        return Err(GatewayError::InvalidRequestBody("model is required".into()));
    }
    match &request.input {
        EmbeddingInput::String(value) if value.trim().is_empty() => Err(
            GatewayError::InvalidRequestBody("input cannot be empty".into()),
        ),
        EmbeddingInput::Array(values) if values.is_empty() => Err(
            GatewayError::InvalidRequestBody("input array cannot be empty".into()),
        ),
        EmbeddingInput::Array(values) if values.iter().any(|value| value.trim().is_empty()) => Err(
            GatewayError::InvalidRequestBody("input array cannot contain empty strings".into()),
        ),
        _ => Ok(()),
    }
}

fn estimate_tokens(request: &ChatCompletionRequest) -> i64 {
    let prompt_chars: usize = request.messages.iter().map(|m| m.content.len()).sum();
    let prompt_tokens = (prompt_chars as i64 / 4).max(1);
    prompt_tokens + i64::from(request.max_tokens.unwrap_or(1024))
}

fn estimate_embedding_tokens(request: &EmbeddingRequest) -> i64 {
    let chars = match &request.input {
        EmbeddingInput::String(value) => value.len(),
        EmbeddingInput::Array(values) => values.iter().map(String::len).sum(),
    };
    (chars as i64 / 4).max(1)
}

fn estimate_embedding_reserve_amount(
    estimated_tokens: i64,
    input_price: &Decimal,
    safety_multiplier: Decimal,
) -> Decimal {
    (Decimal::from(estimated_tokens.max(1)) * *input_price * safety_multiplier
        / Decimal::from(1000u32))
    .round_dp(4)
}

fn estimate_reserve_amount(
    estimated_tokens: i64,
    input_price: &Decimal,
    output_price: &Decimal,
    safety_multiplier: Decimal,
) -> Result<Decimal, GatewayError> {
    let average_price = (*input_price + *output_price) / Decimal::from(2u32);
    Ok(
        (Decimal::from(estimated_tokens.max(1)) * average_price * safety_multiplier
            / Decimal::from(1000u32))
        .round_dp(4),
    )
}

fn encode_mantle_api_key(credentials: &crate::infrastructure::clients::AwsCredentials) -> String {
    let payload = serde_json::json!({
        "access_key_id": credentials.access_key_id,
        "secret_access_key": credentials.secret_access_key,
        "session_token": credentials.session_token,
        "region": credentials.region,
        "expires_at": credentials.expires_at
    });
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string())
}

fn openai_response(model: &str, response: BedrockChatResponse) -> ChatCompletionResponse {
    ChatCompletionResponse {
        id: format!("chatcmpl-{}", Uuid::new_v4()),
        object: "chat.completion".into(),
        created: Utc::now().timestamp(),
        model: model.into(),
        choices: vec![ChatChoice {
            index: 0,
            message: ChatMessageResponse {
                role: "assistant".into(),
                content: response.text,
            },
            finish_reason: response.finish_reason,
        }],
        usage: ChatUsage {
            prompt_tokens: response.usage.input_tokens,
            completion_tokens: response.usage.output_tokens,
            total_tokens: response.usage.input_tokens + response.usage.output_tokens,
        },
    }
}

fn openai_embedding_response(model: &str, response: BedrockEmbeddingResponse) -> EmbeddingResponse {
    EmbeddingResponse {
        object: "list".into(),
        data: response.data,
        model: model.into(),
        usage: EmbeddingUsage {
            prompt_tokens: response.usage.input_tokens,
            total_tokens: response.usage.input_tokens,
        },
    }
}
