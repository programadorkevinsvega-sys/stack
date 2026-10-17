//! Cliente HTTP tipado para Bedrock Mantle (API OpenAI-compatible).
//!
//! Bedrock Mantle expone modelos de Bedrock a través de una API compatible
//! con OpenAI (`/v1/chat/completions`). La autenticación es vía API key
//! que codifica las credenciales AWS como un pre-signed URL.
//!
//! # Invariantes
//! - Nunca expone la API key en logs.
//! - El conteo de tokens se extrae de `usage.prompt_tokens`/`completion_tokens`.
//! - Errores HTTP 429/503 → `RateLimited`/`ServiceUnavailable`.

mod types;

pub use types::*;

use common::DomainError;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use tracing::{debug, error, info_span, Instrument};

/// Credenciales para Bedrock Mantle.
#[derive(Clone)]
pub struct MantleCredentials {
    pub api_key: String,
    pub base_url: String,
}

impl std::fmt::Debug for MantleCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MantleCredentials")
            .field("base_url", &self.base_url)
            .field("api_key", &"[redacted]")
            .finish()
    }
}

/// Cliente HTTP para Bedrock Mantle. Stateless — se puede clonar.
#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
}

impl Client {
    pub fn try_new() -> Result<Self, DomainError> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| {
                DomainError::Internal(format!("failed to build bedrock http client: {}", e))
            })?;
        Ok(Self { http })
    }

    fn build_headers(api_key: &str) -> Result<HeaderMap, DomainError> {
        if api_key.trim().is_empty() {
            return Err(DomainError::Validation(
                "bedrock api key is empty".to_string(),
            ));
        }
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let auth_header = HeaderValue::from_str(&format!("Bearer {}", api_key))
            .map_err(|_| DomainError::Validation("invalid bedrock api key format".to_string()))?;
        headers.insert(AUTHORIZATION, auth_header);
        Ok(headers)
    }

    /// Llamada no-streaming a Bedrock Mantle.
    #[tracing::instrument(skip(self, credentials, request), fields(model = %request.model))]
    pub async fn converse(
        &self,
        credentials: &MantleCredentials,
        request: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, DomainError> {
        let url = format!("{}/v1/chat/completions", credentials.base_url);
        let headers = Self::build_headers(&credentials.api_key)?;
        let start = std::time::Instant::now();

        let resp = self
            .http
            .post(&url)
            .headers(headers)
            .json(request)
            .send()
            .instrument(info_span!("bedrock_mantle.http"))
            .await
            .map_err(|e| {
                error!(error = %e, "Bedrock Mantle HTTP request failed");
                if e.is_timeout() {
                    DomainError::ServiceUnavailable("bedrock request timeout".into())
                } else if e.is_connect() {
                    DomainError::ServiceUnavailable("bedrock connection error".into())
                } else {
                    DomainError::ServiceUnavailable(format!("bedrock http error: {}", e))
                }
            })?;

        let latency_ms = start.elapsed().as_millis() as u64;

        if !resp.status().is_success() {
            let status = resp.status();
            let retry_after_hdr = resp
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            let body = match resp.text().await {
                Ok(body) => body,
                Err(e) => {
                    error!(
                        error = %e,
                        "Failed to read Bedrock Mantle error response body"
                    );
                    "<unavailable-response-body>".to_string()
                }
            };
            error!(status = %status, body = %body, "Bedrock Mantle error response");
            return Err(map_http_error(status.as_u16(), &body, retry_after_hdr));
        }

        let completion: OpenAiChatResponse = resp.json().await.map_err(|e| {
            error!(error = %e, "Failed to parse Bedrock Mantle response");
            DomainError::Internal("failed to parse bedrock response".into())
        })?;

        debug!(
            input_tokens = completion.usage.prompt_tokens,
            output_tokens = completion.usage.completion_tokens,
            latency_ms,
            "Bedrock Mantle converse completed"
        );

        let first_choice = completion
            .choices
            .first()
            .ok_or_else(|| DomainError::Internal("bedrock response has no choices".to_string()))?;

        Ok(ChatCompletionResponse {
            text: first_choice.message.content.clone(),
            usage: TokenUsage {
                input_tokens: non_negative_i32_to_u32(
                    completion.usage.prompt_tokens,
                    "usage.prompt_tokens",
                )?,
                output_tokens: non_negative_i32_to_u32(
                    completion.usage.completion_tokens,
                    "usage.completion_tokens",
                )?,
                cached_tokens: 0,
            },
            finish_reason: first_choice.finish_reason.clone(),
            latency_ms,
        })
    }

    /// Llamada streaming a Bedrock Mantle.
    #[tracing::instrument(skip(self, credentials, request), fields(model = %request.model))]
    pub async fn converse_stream(
        &self,
        credentials: &MantleCredentials,
        request: ChatCompletionRequest,
    ) -> Result<
        impl tokio_stream::Stream<Item = Result<ChatCompletionChunk, DomainError>> + Send + 'static,
        DomainError,
    > {
        let url = format!("{}/v1/chat/completions", credentials.base_url);
        let headers = Self::build_headers(&credentials.api_key)?;
        let body = ChatCompletionRequest {
            stream: true,
            ..request
        };

        let resp = self
            .http
            .post(&url)
            .headers(headers)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                error!(error = %e, "Bedrock Mantle stream request failed");
                DomainError::ServiceUnavailable(format!("bedrock stream error: {}", e))
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let retry_after_hdr = resp
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            let body = match resp.text().await {
                Ok(body) => body,
                Err(e) => {
                    error!(
                        error = %e,
                        "Failed to read Bedrock Mantle stream error response body"
                    );
                    "<unavailable-response-body>".to_string()
                }
            };
            return Err(map_http_error(status.as_u16(), &body, retry_after_hdr));
        }

        use bytes::BytesMut;
        use futures_util::StreamExt;
        use std::sync::Arc;
        use std::sync::Mutex;

        let byte_stream = resp.bytes_stream();
        let buffer = Arc::new(Mutex::new(BytesMut::new()));

        // flat_map approach: each byte chunk is processed and may emit 0..N SSE events
        Ok(byte_stream.flat_map(move |byte_result| {
            let buffer = Arc::clone(&buffer);
            let results: Vec<Result<ChatCompletionChunk, DomainError>> = match byte_result {
                Ok(bytes) => {
                    let mut buf = match buffer.lock() {
                        Ok(guard) => guard,
                        Err(_) => {
                            return futures_util::stream::iter(vec![Err(DomainError::Internal(
                                "stream parser lock poisoned".to_string(),
                            ))]);
                        }
                    };
                    buf.extend_from_slice(&bytes);
                    let mut events = Vec::new();

                    while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                        let line_bytes = buf.split_to(pos + 1);
                        let line = String::from_utf8_lossy(&line_bytes[..line_bytes.len() - 1])
                            .to_string()
                            .trim()
                            .to_string();

                        if let Some(stripped) = line.strip_prefix("data: ") {
                            let data = stripped.trim().to_string();
                            if data == "[DONE]" {
                                continue;
                            }
                            match serde_json::from_str::<OpenAiStreamChunk>(&data) {
                                Ok(chunk) => {
                                    let first_choice = match chunk.choices.first() {
                                        Some(choice) => choice,
                                        None => {
                                            events.push(Err(DomainError::Internal(
                                                "stream chunk has no choices".to_string(),
                                            )));
                                            continue;
                                        }
                                    };
                                    let delta = first_choice.delta.content.clone();
                                    let finish_reason = first_choice.finish_reason.clone();
                                    let usage = match chunk.usage {
                                        Some(u) => {
                                            let input_tokens = match non_negative_i32_to_u32(
                                                u.prompt_tokens,
                                                "stream.usage.prompt_tokens",
                                            ) {
                                                Ok(value) => value,
                                                Err(err) => {
                                                    events.push(Err(err));
                                                    continue;
                                                }
                                            };
                                            let output_tokens = match non_negative_i32_to_u32(
                                                u.completion_tokens,
                                                "stream.usage.completion_tokens",
                                            ) {
                                                Ok(value) => value,
                                                Err(err) => {
                                                    events.push(Err(err));
                                                    continue;
                                                }
                                            };
                                            Some(TokenUsage {
                                                input_tokens,
                                                output_tokens,
                                                cached_tokens: 0,
                                            })
                                        }
                                        None => None,
                                    };
                                    events.push(Ok(ChatCompletionChunk {
                                        delta,
                                        finish_reason,
                                        usage,
                                    }));
                                }
                                Err(e) => {
                                    events.push(Err(DomainError::Internal(format!(
                                        "stream parse error: {}",
                                        e
                                    ))));
                                }
                            }
                        }
                    }
                    events
                }
                Err(e) => {
                    vec![Err(DomainError::Internal(format!(
                        "stream read error: {}",
                        e
                    )))]
                }
            };
            futures_util::stream::iter(results)
        }))
    }

    /// Llamada a embeddings en Bedrock Mantle.
    #[tracing::instrument(skip(self, credentials, request), fields(model = %request.model))]
    pub async fn embeddings(
        &self,
        credentials: &MantleCredentials,
        request: &EmbeddingRequest,
    ) -> Result<EmbeddingResponse, DomainError> {
        let url = format!("{}/v1/embeddings", credentials.base_url);
        let headers = Self::build_headers(&credentials.api_key)?;
        let start = std::time::Instant::now();
        let resp = self
            .http
            .post(&url)
            .headers(headers)
            .json(request)
            .send()
            .instrument(info_span!("bedrock_mantle.embeddings.http"))
            .await
            .map_err(|e| {
                error!(error = %e, "Bedrock Mantle embeddings request failed");
                if e.is_timeout() {
                    DomainError::ServiceUnavailable("bedrock embeddings request timeout".into())
                } else if e.is_connect() {
                    DomainError::ServiceUnavailable("bedrock embeddings connection error".into())
                } else {
                    DomainError::ServiceUnavailable(format!("bedrock embeddings http error: {}", e))
                }
            })?;
        let latency_ms = start.elapsed().as_millis() as u64;
        if !resp.status().is_success() {
            let status = resp.status();
            let retry_after_hdr = resp
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            let body = match resp.text().await {
                Ok(body) => body,
                Err(error) => {
                    error!(%error, "Failed to read Bedrock Mantle embeddings error body");
                    "<unavailable-response-body>".to_string()
                }
            };
            error!(status = %status, body = %body, "Bedrock Mantle embeddings error response");
            return Err(map_http_error(status.as_u16(), &body, retry_after_hdr));
        }
        let embeddings: OpenAiEmbeddingResponse = resp.json().await.map_err(|e| {
            error!(error = %e, "Failed to parse Bedrock Mantle embeddings response");
            DomainError::Internal("failed to parse bedrock embeddings response".into())
        })?;
        debug!(
            input_tokens = embeddings.usage.prompt_tokens,
            latency_ms, "Bedrock Mantle embeddings completed"
        );
        Ok(EmbeddingResponse {
            data: embeddings
                .data
                .into_iter()
                .map(|item| EmbeddingData {
                    object: item.object,
                    embedding: item.embedding,
                    index: item.index,
                })
                .collect(),
            usage: TokenUsage {
                input_tokens: non_negative_i32_to_u32(
                    embeddings.usage.prompt_tokens,
                    "embeddings.usage.prompt_tokens",
                )?,
                output_tokens: 0,
                cached_tokens: 0,
            },
            latency_ms,
        })
    }
}

fn map_http_error(status: u16, body: &str, retry_after_header: Option<u64>) -> DomainError {
    let retry_after = retry_after_header.or_else(|| {
        serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .and_then(|v| {
                v.get("retry_after")
                    .or_else(|| v.pointer("/error/retry_after"))
                    .and_then(|v| v.as_u64())
            })
    });
    match status {
        401 | 403 => DomainError::Forbidden("access denied to model".into()),
        404 => DomainError::NotFound("model not found".into()),
        429 => DomainError::RateLimited {
            retry_after: retry_after.unwrap_or(5),
        },
        500..=599 => {
            DomainError::ServiceUnavailable(format!("model service error: HTTP {}", status))
        }
        400 => DomainError::Validation("invalid request: HTTP 400".to_string()),
        _ => DomainError::Internal(format!("unexpected HTTP {}", status)),
    }
}

fn non_negative_i32_to_u32(value: i32, field: &str) -> Result<u32, DomainError> {
    u32::try_from(value).map_err(|_| {
        DomainError::Internal(format!(
            "invalid negative token count in {}: {}",
            field, value
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_http_error_rate_limited() {
        let err = map_http_error(429, "", None);
        assert!(matches!(err, DomainError::RateLimited { .. }));
    }

    #[test]
    fn test_map_http_error_rate_limited_with_header() {
        let err = map_http_error(429, "", Some(30));
        match err {
            DomainError::RateLimited { retry_after } => assert_eq!(retry_after, 30),
            _ => panic!("expected RateLimited"),
        }
    }

    #[test]
    fn test_map_http_error_rate_limited_with_body() {
        let err = map_http_error(
            429,
            r#"{"error":{"message":"rate limited","retry_after":15}}"#,
            None,
        );
        match err {
            DomainError::RateLimited { retry_after } => assert_eq!(retry_after, 15),
            _ => panic!("expected RateLimited"),
        }
    }

    #[test]
    fn test_map_http_error_rate_limited_header_overrides_body() {
        let err = map_http_error(
            429,
            r#"{"error":{"message":"rate limited","retry_after":15}}"#,
            Some(60),
        );
        match err {
            DomainError::RateLimited { retry_after } => assert_eq!(retry_after, 60),
            _ => panic!("expected RateLimited"),
        }
    }

    #[test]
    fn test_map_http_error_not_found() {
        let err = map_http_error(404, "", None);
        assert!(matches!(err, DomainError::NotFound(_)));
    }
}
