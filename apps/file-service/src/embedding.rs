use futures_util::{stream, StreamExt};
use rand::Rng;
use reqwest::header::{HeaderMap, RETRY_AFTER};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};
use tokenizers::Tokenizer;
use uuid::Uuid;

pub const QWEN_MODEL: &str = "qwen/qwen3-embedding-8b";
pub const QWEN_DIMENSIONS: usize = 4096;
pub const QWEN_MAX_CONTEXT: usize = 32_768;
pub const QWEN_CHUNK_TOKENS: usize = 800;
pub const QWEN_CHUNK_OVERLAP: usize = 120;
pub const QWEN_STRATEGY_VERSION: &str = "v2-qwen-token-800-overlap-120";
pub const TOKENIZER_COMMIT: &str = "4e423935c619ae4df87b646a3ce949610c66241c";
pub const TOKENIZER_URL: &str = "https://huggingface.co/Qwen/Qwen3-Embedding-8B/resolve/4e423935c619ae4df87b646a3ce949610c66241c/tokenizer.json";
pub const TOKENIZER_SHA256: &str =
    "83cdf8c3a34f68862319cb1810ee7b1e2c0a44e0864ae930194ddb76bb7feb8d";
const MAX_TOKENIZER_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone)]
pub struct EmbeddingService {
    tokenizer: Arc<Tokenizer>,
    http: reqwest::Client,
    endpoint: String,
    api_key: Arc<str>,
    model: String,
    dimensions: usize,
    max_input_tokens: usize,
    max_batch_inputs: usize,
    max_batch_tokens: usize,
    max_concurrent_requests: usize,
    max_retries: u32,
    retry_base: Duration,
    retry_max: Duration,
    request_timeout: Duration,
    query_instruction: String,
    referer: Option<String>,
    title: Option<String>,
}

#[derive(Clone, Debug)]
pub struct DocumentChunk {
    pub content: String,
    pub token_count: i32,
}

#[derive(Debug)]
pub struct EmbeddingOutcome {
    pub vectors: Vec<Vec<f32>>,
    pub billed_tokens: i64,
}

#[derive(Debug)]
struct PreparedInput {
    index: usize,
    text: String,
    local_tokens: i64,
}

#[derive(Debug)]
struct BatchOutcome {
    index: usize,
    vectors: Vec<Vec<f32>>,
    billed_tokens: i64,
    provider_reported_tokens: bool,
}

#[derive(Debug, Deserialize)]
struct ProviderResponse {
    data: Vec<ProviderEmbedding>,
    usage: Option<ProviderUsage>,
}

#[derive(Debug, Deserialize)]
struct ProviderEmbedding {
    index: usize,
    embedding: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct ProviderUsage {
    prompt_tokens: Option<i64>,
}

impl EmbeddingService {
    pub async fn from_env() -> anyhow::Result<Self> {
        let endpoint = std::env::var("EMBEDDING_API_URL")
            .map_err(|_| anyhow::anyhow!("EMBEDDING_API_URL is required"))?;
        let api_key = std::env::var("EMBEDDING_API_KEY")
            .map_err(|_| anyhow::anyhow!("EMBEDDING_API_KEY is required from Vault"))?;
        let model = std::env::var("EMBEDDING_MODEL").unwrap_or_else(|_| QWEN_MODEL.into());
        let config = ServiceConfig::from_env(endpoint, api_key, model)?;
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(32)
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()?;
        let tokenizer = load_tokenizer(
            &http,
            Path::new(&config.tokenizer_path),
            &config.tokenizer_url,
            &config.tokenizer_sha256,
        )
        .await?;
        let service = Self {
            tokenizer: Arc::new(tokenizer),
            http,
            endpoint: config.endpoint,
            api_key: Arc::from(config.api_key),
            model: config.model,
            dimensions: config.dimensions,
            max_input_tokens: config.max_input_tokens,
            max_batch_inputs: config.max_batch_inputs,
            max_batch_tokens: config.max_batch_tokens,
            max_concurrent_requests: config.max_concurrent_requests,
            max_retries: config.max_retries,
            retry_base: config.retry_base,
            retry_max: config.retry_max,
            request_timeout: config.request_timeout,
            query_instruction: config.query_instruction,
            referer: config.referer,
            title: config.title,
        };
        tracing::info!(
            model = %service.model,
            dimensions = service.dimensions,
            max_context_tokens = service.max_input_tokens,
            max_batch_inputs = service.max_batch_inputs,
            max_batch_tokens = service.max_batch_tokens,
            max_concurrent_requests = service.max_concurrent_requests,
            tokenizer_commit = TOKENIZER_COMMIT,
            "Qwen embedding provider configured"
        );
        Ok(service)
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn dimensions(&self) -> usize {
        self.dimensions
    }

    pub fn token_count(&self, text: &str) -> Result<i64, String> {
        let encoded = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| format!("tokenization failed: {e}"))?;
        let count = encoded.get_ids().len();
        if count == 0 || count > self.max_input_tokens {
            return Err(format!(
                "input exceeds {} token context",
                self.max_input_tokens
            ));
        }
        i64::try_from(count).map_err(|_| "token count overflow".into())
    }

    pub fn chunk_document(&self, text: &str, strategy: &str) -> Result<Vec<DocumentChunk>, String> {
        let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if normalized.is_empty() {
            return Ok(Vec::new());
        }
        match strategy {
            QWEN_STRATEGY_VERSION => self.chunk_qwen(&normalized),
            "v1-token-800-overlap-120" => self.chunk_legacy(&normalized),
            other => Err(format!("unsupported ingestion strategy: {other}")),
        }
    }

    pub fn format_query(&self, query: &str) -> String {
        format!("Instruct: {}\nQuery:{}", self.query_instruction, query)
    }

    pub async fn embed_documents(&self, inputs: &[String]) -> Result<EmbeddingOutcome, String> {
        self.embed_inputs(inputs).await
    }

    pub async fn embed_query(&self, query: &str) -> Result<EmbeddingOutcome, String> {
        let formatted = self.format_query(query);
        self.embed_inputs(&[formatted]).await
    }

    fn chunk_qwen(&self, normalized: &str) -> Result<Vec<DocumentChunk>, String> {
        let encoded = self
            .tokenizer
            .encode(normalized, false)
            .map_err(|e| format!("tokenization failed: {e}"))?;
        let ids = encoded.get_ids();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let special_count = self
            .tokenizer
            .encode("", true)
            .map_err(|e| format!("tokenization failed: {e}"))?
            .get_ids()
            .len();
        let payload_limit = QWEN_CHUNK_TOKENS
            .checked_sub(special_count)
            .filter(|value| *value > QWEN_CHUNK_OVERLAP)
            .ok_or_else(|| "Qwen tokenizer special-token budget is invalid".to_string())?;
        let mut result = Vec::new();
        let mut start = 0usize;
        while start < ids.len() {
            let mut end = (start + payload_limit).min(ids.len());
            let content = loop {
                let decoded = self
                    .tokenizer
                    .decode(&ids[start..end], true)
                    .map_err(|e| format!("tokenizer decode failed: {e}"))?;
                let count = self.token_count(&decoded)? as usize;
                if count <= QWEN_CHUNK_TOKENS {
                    break (decoded, count);
                }
                if end <= start + 1 {
                    return Err("tokenizer could not produce a bounded chunk".into());
                }
                end -= 1;
            };
            if content.0.trim().is_empty() {
                return Err("tokenizer produced an empty chunk".into());
            }
            result.push(DocumentChunk {
                content: content.0,
                token_count: i32::try_from(content.1)
                    .map_err(|_| "chunk token count overflow".to_string())?,
            });
            if end == ids.len() {
                break;
            }
            let advance = end.saturating_sub(QWEN_CHUNK_OVERLAP);
            if advance <= start {
                return Err("chunk overlap prevented forward progress".into());
            }
            start = advance;
        }
        Ok(result)
    }

    fn chunk_legacy(&self, normalized: &str) -> Result<Vec<DocumentChunk>, String> {
        let words = normalized.split_whitespace().collect::<Vec<_>>();
        let mut result = Vec::new();
        let mut start = 0usize;
        while start < words.len() {
            let end = (start + QWEN_CHUNK_TOKENS).min(words.len());
            let content = words[start..end].join(" ");
            let count = self.token_count(&content)?;
            result.push(DocumentChunk {
                content,
                token_count: i32::try_from(count)
                    .map_err(|_| "chunk token count overflow".to_string())?,
            });
            if end == words.len() {
                break;
            }
            start = end.saturating_sub(QWEN_CHUNK_OVERLAP);
        }
        Ok(result)
    }

    async fn embed_inputs(&self, inputs: &[String]) -> Result<EmbeddingOutcome, String> {
        if inputs.is_empty() {
            return Ok(EmbeddingOutcome {
                vectors: Vec::new(),
                billed_tokens: 0,
            });
        }
        let prepared = inputs
            .iter()
            .enumerate()
            .map(|(index, text)| {
                let local_tokens = self.token_count(text)?;
                Ok(PreparedInput {
                    index,
                    text: text.clone(),
                    local_tokens,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let local_tokens = prepared.iter().map(|item| item.local_tokens).sum::<i64>();
        let batches = plan_batches(prepared, self.max_batch_inputs, self.max_batch_tokens)?;
        metrics::counter!("embedding_batches_total").increment(batches.len() as u64);
        let requests = stream::iter(batches.into_iter().map(|batch| {
            let service = self.clone();
            let batch_start = batch.first().map(|item| item.index).unwrap_or_default();
            async move { service.send_batch(batch_start, batch).await }
        }))
        .buffer_unordered(self.max_concurrent_requests);
        let mut outcomes = requests.collect::<Vec<_>>().await;
        let mut vectors = vec![None; inputs.len()];
        let mut billed_tokens = 0i64;
        let mut provider_reported_tokens = false;
        for outcome in outcomes.drain(..) {
            let outcome = outcome?;
            billed_tokens = billed_tokens.saturating_add(outcome.billed_tokens);
            provider_reported_tokens |= outcome.provider_reported_tokens;
            for (offset, vector) in outcome.vectors.into_iter().enumerate() {
                let index = outcome.index + offset;
                if index >= vectors.len() || vectors[index].is_some() {
                    return Err("embedding response indexes are invalid or duplicated".into());
                }
                vectors[index] = Some(vector);
            }
        }
        let vectors = vectors
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| "embedding response is missing an input vector".to_string())?;
        let effective_billed = if provider_reported_tokens {
            billed_tokens
        } else {
            local_tokens
        };
        metrics::counter!("embedding_local_tokens_total").increment(local_tokens.max(0) as u64);
        metrics::counter!("embedding_billed_tokens_total")
            .increment(effective_billed.max(0) as u64);
        metrics::gauge!("embedding_vector_dimensions").set(self.dimensions as f64);
        Ok(EmbeddingOutcome {
            vectors,
            billed_tokens: effective_billed,
        })
    }

    async fn send_batch(
        &self,
        batch_start: usize,
        batch: Vec<PreparedInput>,
    ) -> Result<BatchOutcome, String> {
        let texts = batch
            .iter()
            .map(|item| item.text.clone())
            .collect::<Vec<_>>();
        let payload = serde_json::json!({
            "model": self.model,
            "input": texts,
            "encoding_format": "float",
            "dimensions": self.dimensions,
        });
        let url = format!("{}/v1/embeddings", self.endpoint.trim_end_matches('/'));
        let started = Instant::now();
        let mut last_error = None;
        for attempt in 0..=self.max_retries {
            let mut request = self
                .http
                .post(&url)
                .bearer_auth(self.api_key.as_ref())
                .timeout(self.request_timeout)
                .json(&payload);
            if let Some(referer) = &self.referer {
                request = request.header("HTTP-Referer", referer);
            }
            if let Some(title) = &self.title {
                request = request.header("X-OpenRouter-Title", title);
            }
            let response = match request.send().await {
                Ok(response) => response,
                Err(error) => {
                    let retryable = error.is_timeout() || error.is_connect();
                    if retryable && attempt < self.max_retries {
                        self.sleep_before_retry(attempt, None).await;
                        continue;
                    }
                    metrics::counter!("embedding_provider_errors_total", "category" => "transport")
                        .increment(1);
                    return Err("embedding provider transport failure".into());
                }
            };
            let status = response.status();
            if status.is_success() {
                let parsed = response
                    .json::<ProviderResponse>()
                    .await
                    .map_err(|_| "embedding provider returned invalid JSON".to_string())?;
                let vectors = parse_vectors(parsed.data, batch.len(), self.dimensions)?;
                let billed = parsed.usage.as_ref().and_then(|usage| usage.prompt_tokens);
                if billed.is_some_and(|value| value < 0) {
                    metrics::counter!("embedding_provider_errors_total", "category" => "invalid_usage")
                        .increment(1);
                    return Err("embedding provider returned invalid token usage".into());
                }
                metrics::histogram!("embedding_provider_request_duration_seconds", "outcome" => "success")
                    .record(started.elapsed().as_secs_f64());
                return Ok(BatchOutcome {
                    index: batch_start,
                    vectors,
                    billed_tokens: billed
                        .unwrap_or_else(|| batch.iter().map(|item| item.local_tokens).sum()),
                    provider_reported_tokens: billed.is_some(),
                });
            }
            let retry_after = retry_after_delay(response.headers(), self.retry_max);
            let retryable = status == reqwest::StatusCode::REQUEST_TIMEOUT
                || status == reqwest::StatusCode::TOO_MANY_REQUESTS
                || status.is_server_error();
            if retryable && attempt < self.max_retries {
                let status_label = if status == reqwest::StatusCode::REQUEST_TIMEOUT {
                    "408"
                } else if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    "429"
                } else {
                    "5xx"
                };
                metrics::counter!("embedding_provider_retries_total", "status" => status_label)
                    .increment(1);
                self.sleep_before_retry(attempt, retry_after).await;
                continue;
            }
            let category = if status.is_client_error() {
                "permanent_4xx"
            } else {
                "provider_5xx"
            };
            metrics::counter!("embedding_provider_errors_total", "category" => category)
                .increment(1);
            last_error = Some(format!("embedding provider returned {status}"));
            break;
        }
        metrics::histogram!("embedding_provider_request_duration_seconds", "outcome" => "error")
            .record(started.elapsed().as_secs_f64());
        Err(last_error.unwrap_or_else(|| "embedding provider request failed".into()))
    }

    async fn sleep_before_retry(&self, attempt: u32, retry_after: Option<Duration>) {
        let exponential = self
            .retry_base
            .checked_mul(1u32.checked_shl(attempt.min(31)).unwrap_or(u32::MAX))
            .unwrap_or(self.retry_max)
            .min(self.retry_max);
        let chosen = retry_after.unwrap_or(exponential).min(self.retry_max);
        let jitter = if chosen.is_zero() {
            Duration::ZERO
        } else {
            let max_jitter = (chosen.as_millis() / 4) as u64;
            Duration::from_millis(if max_jitter == 0 {
                0
            } else {
                rand::thread_rng().gen_range(0..=max_jitter)
            })
        };
        tokio::time::sleep(chosen.saturating_add(jitter)).await;
    }
}

#[derive(Debug)]
struct ServiceConfig {
    endpoint: String,
    api_key: String,
    model: String,
    dimensions: usize,
    max_input_tokens: usize,
    max_batch_inputs: usize,
    max_batch_tokens: usize,
    max_concurrent_requests: usize,
    max_retries: u32,
    retry_base: Duration,
    retry_max: Duration,
    request_timeout: Duration,
    query_instruction: String,
    tokenizer_path: String,
    tokenizer_url: String,
    tokenizer_sha256: String,
    referer: Option<String>,
    title: Option<String>,
}

impl ServiceConfig {
    fn from_env(endpoint: String, api_key: String, model: String) -> anyhow::Result<Self> {
        let parsed = reqwest::Url::parse(&endpoint)?;
        if parsed.scheme() != "https"
            || parsed.host_str() != Some("openrouter.ai")
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            anyhow::bail!("EMBEDDING_API_URL must be the HTTPS OpenRouter API endpoint")
        }
        if api_key.trim().is_empty() {
            anyhow::bail!("EMBEDDING_API_KEY is empty")
        }
        if model != QWEN_MODEL {
            anyhow::bail!("only {QWEN_MODEL} is permitted")
        }
        let dimensions = env_usize("EMBEDDING_DIMENSIONS", QWEN_DIMENSIONS, 1, QWEN_DIMENSIONS)?;
        if dimensions != QWEN_DIMENSIONS {
            anyhow::bail!("Qwen3-Embedding-8B requires {QWEN_DIMENSIONS} dimensions")
        }
        let max_input_tokens = env_usize(
            "EMBEDDING_MAX_INPUT_TOKENS",
            QWEN_MAX_CONTEXT,
            1,
            QWEN_MAX_CONTEXT,
        )?;
        let max_batch_inputs = env_usize("EMBEDDING_MAX_BATCH_INPUTS", 16, 1, 128)?;
        let max_batch_tokens = env_usize(
            "EMBEDDING_MAX_BATCH_TOKENS",
            QWEN_MAX_CONTEXT,
            1,
            max_input_tokens,
        )?;
        let max_concurrent_requests = env_usize("EMBEDDING_MAX_CONCURRENT_REQUESTS", 4, 1, 32)?;
        let max_retries = env_u32("EMBEDDING_MAX_RETRIES", 3, 0, 10)?;
        let retry_base_ms = env_u64("EMBEDDING_RETRY_BASE_MS", 250, 1, 60_000)?;
        let retry_max_ms = env_u64("EMBEDDING_RETRY_MAX_MS", 4_000, retry_base_ms, 300_000)?;
        let timeout_seconds = env_u64("EMBEDDING_REQUEST_TIMEOUT_SECONDS", 120, 1, 900)?;
        let tokenizer_path = std::env::var("EMBEDDING_TOKENIZER_PATH")
            .unwrap_or_else(|_| ".runtime/tokenizers/qwen3-embedding-8b-tokenizer.json".into());
        let tokenizer_url =
            std::env::var("EMBEDDING_TOKENIZER_URL").unwrap_or_else(|_| TOKENIZER_URL.into());
        let tokenizer_sha256 =
            std::env::var("EMBEDDING_TOKENIZER_SHA256").unwrap_or_else(|_| TOKENIZER_SHA256.into());
        if tokenizer_url != TOKENIZER_URL || tokenizer_sha256 != TOKENIZER_SHA256 {
            anyhow::bail!("Qwen tokenizer URL and SHA-256 are pinned and cannot be overridden")
        }
        let query_instruction = std::env::var("EMBEDDING_QUERY_INSTRUCTION").unwrap_or_else(|_| {
            "Given a user query, retrieve relevant passages that answer the query".into()
        });
        if query_instruction.trim().is_empty()
            || query_instruction.len() > 1024
            || query_instruction.contains('\0')
        {
            anyhow::bail!("EMBEDDING_QUERY_INSTRUCTION is invalid")
        }
        Ok(Self {
            endpoint: endpoint.trim_end_matches('/').into(),
            api_key,
            model,
            dimensions,
            max_input_tokens,
            max_batch_inputs,
            max_batch_tokens,
            max_concurrent_requests,
            max_retries,
            retry_base: Duration::from_millis(retry_base_ms),
            retry_max: Duration::from_millis(retry_max_ms),
            request_timeout: Duration::from_secs(timeout_seconds),
            query_instruction,
            tokenizer_path,
            tokenizer_url,
            tokenizer_sha256,
            referer: optional_safe_env("EMBEDDING_HTTP_REFERER")?,
            title: optional_safe_env("EMBEDDING_OPENROUTER_TITLE")?,
        })
    }
}

fn env_usize(name: &str, default: usize, min: usize, max: usize) -> anyhow::Result<usize> {
    let value = std::env::var(name)
        .ok()
        .map(|v| v.parse::<usize>())
        .transpose()?
        .unwrap_or(default);
    if !(min..=max).contains(&value) {
        anyhow::bail!("{name} must be between {min} and {max}");
    }
    Ok(value)
}

fn env_u64(name: &str, default: u64, min: u64, max: u64) -> anyhow::Result<u64> {
    let value = std::env::var(name)
        .ok()
        .map(|v| v.parse::<u64>())
        .transpose()?
        .unwrap_or(default);
    if !(min..=max).contains(&value) {
        anyhow::bail!("{name} must be between {min} and {max}");
    }
    Ok(value)
}

fn env_u32(name: &str, default: u32, min: u32, max: u32) -> anyhow::Result<u32> {
    let value = std::env::var(name)
        .ok()
        .map(|v| v.parse::<u32>())
        .transpose()?
        .unwrap_or(default);
    if !(min..=max).contains(&value) {
        anyhow::bail!("{name} must be between {min} and {max}");
    }
    Ok(value)
}

fn optional_safe_env(name: &str) -> anyhow::Result<Option<String>> {
    let value = std::env::var(name).ok().filter(|v| !v.trim().is_empty());
    if value.as_deref().is_some_and(|v| v.contains(['\r', '\n'])) {
        anyhow::bail!("{name} contains invalid header characters");
    }
    Ok(value)
}

fn plan_batches(
    inputs: Vec<PreparedInput>,
    max_inputs: usize,
    max_tokens: usize,
) -> Result<Vec<Vec<PreparedInput>>, String> {
    let mut batches = Vec::new();
    let mut current = Vec::new();
    let mut current_tokens = 0usize;
    for input in inputs {
        let tokens =
            usize::try_from(input.local_tokens).map_err(|_| "token count overflow".to_string())?;
        if tokens > max_tokens {
            return Err("input exceeds embedding batch token limit".into());
        }
        if !current.is_empty()
            && (current.len() >= max_inputs || current_tokens.saturating_add(tokens) > max_tokens)
        {
            batches.push(std::mem::take(&mut current));
            current_tokens = 0;
        }
        current_tokens = current_tokens.saturating_add(tokens);
        current.push(input);
    }
    if !current.is_empty() {
        batches.push(current);
    }
    Ok(batches)
}

fn parse_vectors(
    data: Vec<ProviderEmbedding>,
    expected: usize,
    dimensions: usize,
) -> Result<Vec<Vec<f32>>, String> {
    if data.len() != expected {
        return Err("embedding provider response count mismatch".into());
    }
    let mut ordered = vec![None; expected];
    for item in data {
        if item.index >= expected || ordered[item.index].is_some() {
            return Err("embedding provider response indexes are invalid".into());
        }
        if item.embedding.len() != dimensions
            || item.embedding.iter().any(|value| !value.is_finite())
        {
            metrics::counter!("embedding_provider_errors_total", "category" => "invalid_vector")
                .increment(1);
            return Err("embedding provider returned an invalid vector dimension or value".into());
        }
        ordered[item.index] = Some(item.embedding);
    }
    ordered
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| "embedding vector response is incomplete".into())
}

fn retry_after_delay(headers: &HeaderMap, max: Duration) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    let delay = value
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
        .or_else(|| {
            httpdate::parse_http_date(value)
                .ok()?
                .duration_since(SystemTime::now())
                .ok()
        })?;
    Some(delay.min(max))
}

async fn load_tokenizer(
    http: &reqwest::Client,
    path: &Path,
    url: &str,
    expected_sha: &str,
) -> anyhow::Result<Tokenizer> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    if tokio::fs::symlink_metadata(path)
        .await
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(false)
    {
        anyhow::bail!("Qwen tokenizer path must not be a symlink")
    }
    let bytes = match tokio::fs::read(path).await {
        Ok(bytes) => {
            verify_tokenizer(&bytes, expected_sha)?;
            bytes
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let response = http
                .get(url)
                .timeout(Duration::from_secs(120))
                .send()
                .await?;
            if !response.status().is_success() {
                anyhow::bail!("Qwen tokenizer download returned {}", response.status());
            }
            if response
                .content_length()
                .is_some_and(|length| length as usize > MAX_TOKENIZER_BYTES)
            {
                anyhow::bail!("Qwen tokenizer exceeds the maximum artifact size");
            }
            let mut bytes = Vec::new();
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                if bytes.len().saturating_add(chunk.len()) > MAX_TOKENIZER_BYTES {
                    anyhow::bail!("Qwen tokenizer exceeds the maximum artifact size");
                }
                bytes.extend_from_slice(&chunk);
            }
            verify_tokenizer(&bytes, expected_sha)?;
            let temp = path.with_extension(format!("download-{}", Uuid::now_v7()));
            tokio::fs::write(&temp, &bytes).await?;
            if let Err(rename_error) = tokio::fs::rename(&temp, path).await {
                let _ = tokio::fs::remove_file(&temp).await;
                if tokio::fs::metadata(path).await.is_err() {
                    return Err(rename_error.into());
                }
            }
            bytes
        }
        Err(error) => return Err(error.into()),
    };
    tokio::task::spawn_blocking(move || {
        Tokenizer::from_bytes(bytes).map_err(|e| anyhow::anyhow!("invalid Qwen tokenizer: {e}"))
    })
    .await?
}

fn verify_tokenizer(bytes: &[u8], expected_sha: &str) -> anyhow::Result<()> {
    let digest = hex::encode(Sha256::digest(bytes));
    if digest != expected_sha {
        anyhow::bail!("Qwen tokenizer SHA-256 mismatch")
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qwen_contract_is_pinned() {
        assert_eq!(QWEN_MODEL, "qwen/qwen3-embedding-8b");
        assert_eq!(QWEN_DIMENSIONS, 4096);
        assert_eq!(QWEN_MAX_CONTEXT, 32_768);
        assert_eq!(TOKENIZER_SHA256.len(), 64);
        assert_eq!(TOKENIZER_COMMIT.len(), 40);
    }

    #[test]
    fn planner_respects_input_and_token_limits() {
        let inputs = (0..5)
            .map(|index| PreparedInput {
                index,
                text: index.to_string(),
                local_tokens: 3,
            })
            .collect();
        let batches = plan_batches(inputs, 2, 6).unwrap();
        assert_eq!(batches.len(), 3);
        assert!(batches.iter().all(|batch| batch.len() <= 2));
    }

    #[test]
    fn planner_rejects_single_input_over_batch_budget() {
        let inputs = vec![PreparedInput {
            index: 0,
            text: "x".into(),
            local_tokens: 7,
        }];
        assert!(plan_batches(inputs, 2, 6).is_err());
    }

    #[test]
    fn parser_rejects_wrong_dimensions_and_accepts_ordered_response() {
        let data = vec![
            ProviderEmbedding {
                index: 1,
                embedding: vec![0.0, 1.0],
            },
            ProviderEmbedding {
                index: 0,
                embedding: vec![1.0, 0.0],
            },
        ];
        let parsed = parse_vectors(data, 2, 2).unwrap();
        assert_eq!(parsed[0], vec![1.0, 0.0]);
        assert!(parse_vectors(
            vec![ProviderEmbedding {
                index: 0,
                embedding: vec![f32::NAN]
            }],
            1,
            1
        )
        .is_err());
    }

    #[tokio::test]
    #[ignore = "downloads the pinned public tokenizer artifact"]
    async fn official_tokenizer_artifact_loads_and_tokenizes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("tokenizer.json");
        let tokenizer = load_tokenizer(
            &reqwest::Client::new(),
            &path,
            TOKENIZER_URL,
            TOKENIZER_SHA256,
        )
        .await
        .unwrap();
        assert!(!tokenizer
            .encode("Nexus retrieval", true)
            .unwrap()
            .get_ids()
            .is_empty());
    }
}
