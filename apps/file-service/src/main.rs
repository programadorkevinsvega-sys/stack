mod canonical;
mod embedding;
mod quota;

use anyhow::Context;
use aws_credential_types::Credentials;
use aws_sdk_s3::presigning::PresigningConfig;
use axum::{extract::State, routing::get, Router};
use common::{derive_kid, AppConfig, ServiceJwtAuth, ServiceJwtClaims, ServiceTlsConfig};
use events::{
    jetstream::{
        ack, delivery_attempts, ensure_streams, pull_consumer_config, retry_or_dead_letter,
        DEFAULT_MAX_DELIVERIES, EVENTS_STREAM,
    },
    outbox::enqueue_in_transaction,
    EventEnvelope, FileIngestRequested, FileVectorsDeleteRequested, EVENT_FILE_INGEST_REQUESTED,
    EVENT_FILE_VECTORS_DELETE_REQUESTED,
};
use futures_util::StreamExt;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use proto::file_v1::{
    file_service_server::{FileService, FileServiceServer},
    *,
};
use proto::plan_resolution_v1::plan_resolution_service_client::PlanResolutionServiceClient;
use proto::plan_resolution_v1::GetPlanLimitsRequest;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::io::{Cursor, Read};
use std::time::Duration;
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::Arc,
};
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, Endpoint};
use tonic::{Request, Response, Status};
use tracing::Instrument;
use uuid::Uuid;

#[derive(Clone)]
struct App {
    pool: PgPool,
    plan_client: PlanClient,
    s3: aws_sdk_s3::Client,
    bucket: String,
    max_file_bytes: i64,
    upload_url_ttl_seconds: u64,
    embedding_model: String,
    ingestion_strategy_version: String,
    embedding: Arc<embedding::EmbeddingService>,
    embedding_cost_micros_per_1k: i64,
    embedding_http: reqwest::Client,
    qdrant_url: String,
    qdrant_api_key: Option<String>,
    qdrant_collection: String,
    embedding_version: String,
    embedding_dedup_lease_seconds: i32,
    embedding_max_file_filter_ids: i64,
    dual_write_v1: bool,
    vector_read_mode: String,
    legacy_qdrant_collection: String,
    qdrant_ready_collections: Arc<tokio::sync::RwLock<HashSet<String>>>,
    quota: quota::QuotaManager,
}

impl App {
    /// Refresh low-cardinality operational gauges from PostgreSQL.  These
    /// gauges deliberately do not include user identifiers so the metrics
    /// endpoint cannot become a source of tenant PII or unbounded labels.
    async fn refresh_observability_metrics(&self) -> Result<(), String> {
        let vectors = sqlx::query_as::<_, (String, i64, i64)>(
            "SELECT status, COUNT(*)::BIGINT, COALESCE(SUM(vector_bytes),0)::BIGINT
             FROM embedding_vectors GROUP BY status",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| error.to_string())?;
        for (status, count, bytes) in vectors {
            metrics::gauge!("canonical_embedding_vectors", "status" => status.clone())
                .set(count as f64);
            if status == "ready" {
                metrics::gauge!("canonical_embedding_bytes").set(bytes as f64);
            }
        }
        let references: i64 =
            sqlx::query_scalar("SELECT COUNT(*)::BIGINT FROM embedding_chunk_refs")
                .fetch_one(&self.pool)
                .await
                .map_err(|error| error.to_string())?;
        let orphans: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)::BIGINT FROM embedding_vectors WHERE status='ready' AND reference_count=0",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|error| error.to_string())?;
        metrics::gauge!("canonical_embedding_references").set(references as f64);
        metrics::gauge!("canonical_embedding_orphans").set(orphans as f64);

        let jobs = sqlx::query_as::<_, (String, i64)>(
            "SELECT status, COUNT(*)::BIGINT FROM ingestion_jobs GROUP BY status",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| error.to_string())?;
        for (status, count) in jobs {
            metrics::gauge!("file_ingestion_jobs", "status" => status).set(count as f64);
        }
        let pending_reservations: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)::BIGINT FROM ingestion_quota_reservations WHERE status='reserved'",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|error| error.to_string())?;
        let concurrent_reservations: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)::BIGINT FROM ingestion_quota_reservations WHERE status='processing'",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|error| error.to_string())?;
        metrics::gauge!("file_ingestion_quota_reservations", "status" => "reserved")
            .set(pending_reservations as f64);
        metrics::gauge!("file_ingestion_quota_reservations", "status" => "processing")
            .set(concurrent_reservations as f64);
        let outbox_pending: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)::BIGINT FROM event_outbox WHERE status IN ('pending','processing','failed')",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|error| error.to_string())?;
        metrics::gauge!("file_event_outbox_pending").set(outbox_pending as f64);
        Ok(())
    }

    async fn mark_ingestion_failure(
        &self,
        event: &FileIngestRequested,
        error: &str,
        dead_letter: bool,
    ) {
        let status = if dead_letter { "dead_letter" } else { "failed" };
        if let Err(db_error) = sqlx::query("UPDATE ingestion_jobs SET status=$1,last_error=$2,updated_at=now() WHERE file_id=$3 AND strategy_version=$4 AND embedding_model=$5 AND status <> 'completed'")
            .bind(status)
            .bind(error)
            .bind(event.file_id)
            .bind(&event.strategy_version)
            .bind(&event.embedding_model)
            .execute(&self.pool)
            .await
        {
            tracing::error!(file_id=%event.file_id, error=%db_error, "failed to persist ingestion failure status");
        }
    }

    async fn write_legacy_projection(
        &self,
        user: Uuid,
        file: Uuid,
        strategy_version: &str,
        model: &str,
        chunks: &[canonical::ChunkReference],
        vectors: &HashMap<Uuid, canonical::CanonicalVector>,
    ) -> Result<(), String> {
        let mut tx = self.pool.begin().await.map_err(|error| error.to_string())?;
        for chunk in chunks {
            let vector = vectors
                .get(&chunk.vector_id)
                .ok_or_else(|| "legacy projection vector mapping is incomplete".to_string())?;
            let embedding = vector
                .embedding
                .clone()
                .ok_or_else(|| "legacy projection vector is missing embedding".to_string())?;
            sqlx::query(
                "INSERT INTO embedding_chunks(
                    id,file_id,user_id,document_id,chunk_id,chunk_index,
                    strategy_version,embedding_model,content,content_hash,embedding,
                    token_count,embedding_version,vector_bytes,collection_name
                 ) VALUES($1,$2,$3,$2,$1,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
                 ON CONFLICT(file_id,strategy_version,embedding_model,chunk_index)
                 DO UPDATE SET content=EXCLUDED.content,content_hash=EXCLUDED.content_hash,
                    embedding=EXCLUDED.embedding,token_count=EXCLUDED.token_count,
                    embedding_version=EXCLUDED.embedding_version,vector_bytes=EXCLUDED.vector_bytes,
                    collection_name=EXCLUDED.collection_name",
            )
            .bind(chunk.chunk_id)
            .bind(file)
            .bind(user)
            .bind(chunk.chunk_index)
            .bind(strategy_version)
            .bind(model)
            .bind(&vector.content)
            .bind(&vector.content_hash)
            .bind(embedding)
            .bind(vector.token_count)
            .bind(&vector.embedding_version)
            .bind(vector.vector_bytes)
            .bind(&vector.collection_name)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
        }
        tx.commit().await.map_err(|error| error.to_string())?;
        let base = self.qdrant_url.trim_end_matches('/');
        let points = chunks
            .iter()
            .map(|chunk| {
                let vector = vectors
                    .get(&chunk.vector_id)
                    .ok_or_else(|| "legacy Qdrant vector mapping is incomplete".to_string())?;
                let embedding = vector
                    .embedding
                    .clone()
                    .ok_or_else(|| "legacy Qdrant vector is missing embedding".to_string())?;
                Ok(serde_json::json!({
                    "id": chunk.chunk_id,
                    "vector": embedding,
                    "payload": {
                        "user_id": user.to_string(),
                        "file_id": file.to_string(),
                        "document_id": file.to_string(),
                        "chunk_id": chunk.chunk_id.to_string(),
                        "content_hash": vector.content_hash.clone(),
                        "embedding_model": model,
                        "embedding_version": vector.embedding_version.clone()
                    }
                }))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut request = self
            .embedding_http
            .put(format!(
                "{base}/collections/{}/points?wait=true",
                self.legacy_qdrant_collection
            ))
            .json(&serde_json::json!({"points": points}));
        if let Some(key) = &self.qdrant_api_key {
            request = request.header("api-key", key);
        }
        let response = request
            .send()
            .await
            .map_err(|error| format!("legacy Qdrant dual-write failed: {error}"))?;
        if !response.status().is_success() {
            return Err(format!(
                "legacy Qdrant dual-write returned {}",
                response.status()
            ));
        }
        metrics::counter!("vector_dual_write_v1_total").increment(chunks.len() as u64);
        Ok(())
    }

    async fn process_ingestion(&self, event: FileIngestRequested) -> Result<(), String> {
        let limits = self
            .plan_limits(event.user_id)
            .await
            .map_err(|e| e.message().to_string())?;
        let plan_label = if limits.plan_id.trim().is_empty() {
            limits.plan_name.clone()
        } else {
            limits.plan_id.clone()
        };
        let user_hash = common::observability::hash_user_id(event.user_id);
        if !limits.embeddings_enabled {
            return Err("embeddings are disabled for the active plan".into());
        }
        if !limits.rag_enabled || limits.qdrant_mode == "none" {
            return Err("vector storage is disabled for the active plan".into());
        }
        if limits.max_file_size_bytes > 0 && event.size_bytes > limits.max_file_size_bytes {
            return Err("file exceeds the active plan embedding limit".into());
        }
        if event.embedding_model != self.embedding_model {
            return Err(
                "ingestion event attempted to override the configured embedding model".into(),
            );
        }
        let reservation_id = event
            .quota_reservation_id
            .ok_or_else(|| "ingestion event is missing its quota reservation".to_owned())?;
        let reservation_status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM ingestion_quota_reservations WHERE id=$1 AND user_id=$2",
        )
        .bind(reservation_id)
        .bind(event.user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| error.to_string())?;
        if reservation_status.as_deref() == Some("completed") {
            return Ok(());
        }
        if reservation_status.as_deref() == Some("released") {
            return Err("ingestion quota reservation is no longer active".into());
        }
        let quota_limits = Self::ingestion_limits(&limits);
        if !self
            .quota
            .begin(event.user_id, reservation_id, quota_limits)
            .await?
        {
            return Err("ingestion concurrency limit reached".into());
        }
        let job_id = Uuid::now_v7();
        let inserted = sqlx::query("INSERT INTO ingestion_jobs(id,file_id,user_id,status,strategy_version,embedding_model) VALUES($1,$2,$3,'pending',$4,$5) ON CONFLICT(file_id,strategy_version,embedding_model) DO NOTHING")
            .bind(job_id).bind(event.file_id).bind(event.user_id).bind(&event.strategy_version).bind(&event.embedding_model)
            .execute(&self.pool).await.map_err(|e| e.to_string())?;
        let job = sqlx::query_as::<_, (Uuid, String, i32)>("SELECT id,status,attempts FROM ingestion_jobs WHERE file_id=$1 AND strategy_version=$2 AND embedding_model=$3")
            .bind(event.file_id).bind(&event.strategy_version).bind(&event.embedding_model).fetch_one(&self.pool).await.map_err(|e| e.to_string())?;
        if inserted.rows_affected() == 0 && job.1 == "completed" {
            self.quota
                .finish(event.user_id, reservation_id, 0, true, None)
                .await?;
            return Ok(());
        }
        sqlx::query("UPDATE ingestion_jobs SET status='processing',attempts=attempts+1,started_at=now(),updated_at=now(),last_error=NULL WHERE id=$1")
            .bind(job.0).execute(&self.pool).await.map_err(|e| e.to_string())?;
        let object = self
            .s3
            .get_object()
            .bucket(&self.bucket)
            .key(&event.object_key)
            .send()
            .await
            .map_err(|e| format!("R2 read failed: {e}"))?;
        let mut reader = object.body.into_async_read();
        let mut bytes = Vec::with_capacity(event.size_bytes.min(8 * 1024 * 1024) as usize);
        let mut buf = [0u8; 1024 * 1024];
        let mut total = 0i64;
        loop {
            let n = reader
                .read(&mut buf)
                .await
                .map_err(|e| format!("R2 stream failed: {e}"))?;
            if n == 0 {
                break;
            }
            total += n as i64;
            if total > self.max_file_bytes {
                return Err("source exceeds FILE_SERVICE_MAX_FILE_SIZE_BYTES".into());
            }
            bytes.extend_from_slice(&buf[..n]);
        }
        let source_sha256 = hex::encode(Sha256::digest(&bytes));
        sqlx::query("UPDATE ingestion_jobs SET source_sha256=$1,updated_at=now() WHERE id=$2")
            .bind(&source_sha256)
            .bind(job.0)
            .execute(&self.pool)
            .await
            .map_err(|error| error.to_string())?;
        let text = extract_document_text(&event.mime_type, &bytes)?;
        let chunks = self
            .embedding
            .chunk_document(&text, &event.strategy_version)?;
        if chunks.is_empty() {
            return Err("document has no extractable text".into());
        }
        let collection = self.collection_for(event.user_id, &limits.qdrant_mode)?;
        let mut unique_by_hash = HashMap::<String, canonical::UniqueChunk>::new();
        let mut chunk_hashes = Vec::with_capacity(chunks.len());
        for chunk in &chunks {
            let hash = hex::encode(Sha256::digest(chunk.content.as_bytes()));
            chunk_hashes.push(hash.clone());
            unique_by_hash
                .entry(hash.clone())
                .or_insert_with(|| canonical::UniqueChunk {
                    content_hash: hash,
                    content: chunk.content.clone(),
                    token_count: chunk.token_count,
                });
        }
        let mut unique_chunks = unique_by_hash.into_values().collect::<Vec<_>>();
        unique_chunks.sort_by(|left, right| left.content_hash.cmp(&right.content_hash));
        let strategy_family = strategy_family(&event.strategy_version);
        let reservation = canonical::reserve_vectors(canonical::ReserveVectorsInput {
            pool: &self.pool,
            user_id: event.user_id,
            embedding_model: &event.embedding_model,
            embedding_version: &self.embedding_version,
            strategy_family: &strategy_family,
            collection_name: &collection,
            chunks: &unique_chunks,
            lease_seconds: self.embedding_dedup_lease_seconds,
        })
        .await?;

        let owned_inputs = reservation
            .owned
            .iter()
            .map(|vector| vector.content.clone())
            .collect::<Vec<_>>();
        let (generated_vectors, input_tokens) = if owned_inputs.is_empty() {
            (Vec::new(), 0)
        } else {
            let span = tracing::info_span!(
                "embedding.provider",
                otel.kind = "client",
                provider = "openrouter",
                model = %event.embedding_model,
                user_hash = %user_hash,
                batch_size = owned_inputs.len()
            );
            match self
                .embedding
                .embed_documents(&owned_inputs)
                .instrument(span)
                .await
            {
                Ok(result) => (result.vectors, result.billed_tokens),
                Err(error) => {
                    if let Err(mark_error) =
                        canonical::mark_owned_failed(&self.pool, reservation.worker_id, &error)
                            .await
                    {
                        tracing::error!(
                            error = %mark_error,
                            worker_id = %reservation.worker_id,
                            "failed to release canonical embedding leases"
                        );
                    }
                    return Err(error);
                }
            }
        };
        if generated_vectors.len() != reservation.owned.len() {
            canonical::mark_owned_failed(
                &self.pool,
                reservation.worker_id,
                "embedding response count mismatch",
            )
            .await
            .map_err(|error| error.to_string())?;
            return Err("embedding response count mismatch".into());
        }
        let completed = reservation
            .owned
            .iter()
            .zip(generated_vectors)
            .map(|(row, vector)| (row.id, vector))
            .collect::<Vec<_>>();
        if let Err(error) =
            canonical::complete_owned_vectors(&self.pool, reservation.worker_id, &completed).await
        {
            canonical::mark_owned_failed(&self.pool, reservation.worker_id, &error)
                .await
                .map_err(|db_error| db_error.to_string())?;
            return Err(error);
        }

        let unique_hashes = unique_chunks
            .iter()
            .map(|chunk| chunk.content_hash.clone())
            .collect::<Vec<_>>();
        let canonical_vectors = canonical::load_ready_vectors(
            &self.pool,
            event.user_id,
            &event.embedding_model,
            &self.embedding_version,
            &unique_hashes,
        )
        .await?;
        let dimensions = canonical_vectors
            .iter()
            .map(|vector| vector.vector_dimension.unwrap_or_default())
            .collect::<HashSet<_>>();
        if dimensions.len() != 1 || dimensions.contains(&0) {
            return Err("canonical vectors have inconsistent dimensions".into());
        }
        let stored_dimensions = dimensions.into_iter().next().unwrap() as usize;
        if stored_dimensions != self.embedding.dimensions() {
            return Err(
                "canonical vector dimension does not match configured Qwen dimension".into(),
            );
        }
        self.ensure_qdrant_collection(&collection).await?;
        let pending_sync = canonical_vectors
            .iter()
            .filter(|vector| !vector.qdrant_synced || vector.collection_name != collection)
            .cloned()
            .collect::<Vec<_>>();
        if !pending_sync.is_empty() {
            let span = tracing::info_span!(
                "qdrant.upsert",
                otel.kind = "client",
                collection = %collection,
                user_hash = %user_hash,
                vector_count = pending_sync.len()
            );
            self.upsert_canonical_qdrant(&collection, event.user_id, &pending_sync)
                .instrument(span)
                .await?;
            let ids = pending_sync
                .iter()
                .map(|vector| vector.id)
                .collect::<Vec<_>>();
            canonical::mark_qdrant_synced(&self.pool, event.user_id, &collection, &ids).await?;
        }

        let vector_by_hash = canonical_vectors
            .iter()
            .map(|vector| (vector.content_hash.as_str(), vector))
            .collect::<HashMap<_, _>>();
        let mut references = Vec::with_capacity(chunks.len());
        for (index, hash) in chunk_hashes.iter().enumerate() {
            let vector = vector_by_hash
                .get(hash.as_str())
                .ok_or_else(|| "canonical vector mapping is incomplete".to_string())?;
            let chunk_id = Uuid::new_v5(
                &event.file_id,
                format!("{}:{index}", self.embedding_version).as_bytes(),
            );
            references.push(canonical::ChunkReference {
                chunk_id,
                chunk_index: index as i32,
                vector_id: vector.id,
            });
        }
        let cache_misses = reservation.owned.len() as i64;
        let cache_hits = chunks.len() as i64 - cache_misses;
        let total_estimated_tokens = chunk_hashes
            .iter()
            .filter_map(|hash| vector_by_hash.get(hash.as_str()))
            .map(|vector| i64::from(vector.token_count))
            .sum::<i64>();
        let generated_estimated_tokens = reservation
            .owned
            .iter()
            .map(|vector| i64::from(vector.token_count))
            .sum::<i64>();
        let tokens_avoided = total_estimated_tokens
            .saturating_sub(generated_estimated_tokens)
            .max(0);
        let cost_micros = input_tokens.saturating_mul(self.embedding_cost_micros_per_1k) / 1000;
        let cost_avoided_micros =
            tokens_avoided.saturating_mul(self.embedding_cost_micros_per_1k) / 1000;
        if self.dual_write_v1 {
            let vectors_by_id = canonical_vectors
                .iter()
                .cloned()
                .map(|vector| (vector.id, vector))
                .collect::<HashMap<_, _>>();
            self.write_legacy_projection(
                event.user_id,
                event.file_id,
                &event.strategy_version,
                &event.embedding_model,
                &references,
                &vectors_by_id,
            )
            .await?;
        }
        let reference_stats = canonical::commit_file_references(
            &self.pool,
            &references,
            canonical::FileReferenceCommit {
                user_id: event.user_id,
                file_id: event.file_id,
                strategy_version: &event.strategy_version,
                embedding_model: &event.embedding_model,
                embedding_version: &self.embedding_version,
                job_id: job.0,
                accounting: canonical::IngestionAccounting {
                    embedding_count: chunks.len() as i32,
                    input_tokens,
                    cost_micros,
                    cache_hits,
                    cache_misses,
                    tokens_avoided,
                    cost_avoided_micros,
                },
            },
        )
        .await?;
        self.flush_pending_canonical_deletions(event.user_id)
            .await?;
        metrics::counter!("embedding_cache_hits_total", "plan" => plan_label.clone(), "user_hash" => user_hash.clone())
            .increment(cache_hits.max(0) as u64);
        metrics::counter!("embedding_cache_misses_total", "plan" => plan_label.clone(), "user_hash" => user_hash.clone())
            .increment(cache_misses.max(0) as u64);
        metrics::counter!("embedding_tokens_avoided_total", "plan" => plan_label.clone(), "user_hash" => user_hash.clone())
            .increment(tokens_avoided.max(0) as u64);
        metrics::counter!("embedding_cost_micros_total", "plan" => plan_label.clone(), "user_hash" => user_hash.clone())
            .increment(cost_micros.max(0) as u64);
        metrics::counter!("embedding_cost_avoided_micros_total", "plan" => plan_label.clone(), "user_hash" => user_hash.clone())
            .increment(cost_avoided_micros.max(0) as u64);
        metrics::counter!("embedding_operations_total", "plan" => plan_label, "result" => "completed", "user_hash" => user_hash.clone())
            .increment(1);
        metrics::counter!("embedding_chunks_retained_total")
            .increment(reference_stats.retained.max(0) as u64);
        metrics::counter!("embedding_chunks_added_total")
            .increment(reference_stats.added.max(0) as u64);
        metrics::counter!("embedding_chunks_removed_total")
            .increment(reference_stats.removed.max(0) as u64);
        tracing::info!(
            file_id=%event.file_id,
            user_id=%event.user_id,
            chunks=chunks.len(),
            unique_vectors=canonical_vectors.len(),
            cache_hits,
            cache_misses,
            tokens_avoided,
            input_tokens,
            cost_micros,
            cost_avoided_micros,
            chunks_retained=reference_stats.retained,
            chunks_added=reference_stats.added,
            chunks_removed=reference_stats.removed,
            source_sha256=%source_sha256,
            "file ingestion completed with canonical embedding deduplication"
        );
        self.quota
            .finish(event.user_id, reservation_id, input_tokens, true, None)
            .await?;
        Ok(())
    }

    fn collection_for(&self, user: Uuid, mode: &str) -> Result<String, String> {
        qdrant_collection_name(&self.qdrant_collection, user, mode)
    }

    async fn ensure_qdrant_collection(&self, collection: &str) -> Result<(), String> {
        if self
            .qdrant_ready_collections
            .read()
            .await
            .contains(collection)
        {
            return Ok(());
        }
        let base = self.qdrant_url.trim_end_matches('/');
        let mut inspect = self
            .embedding_http
            .get(format!("{base}/collections/{collection}"));
        if let Some(key) = &self.qdrant_api_key {
            inspect = inspect.header("api-key", key);
        }
        let inspect_response = inspect
            .send()
            .await
            .map_err(|e| format!("Qdrant unavailable: {e}"))?;
        if inspect_response.status().is_success() {
            let value: serde_json::Value = inspect_response
                .json()
                .await
                .map_err(|e| format!("invalid Qdrant collection metadata: {e}"))?;
            let dimensions = value
                .pointer("/result/config/params/vectors/size")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| "Qdrant collection has no scalar vector dimension".to_string())?
                as usize;
            if dimensions != self.embedding.dimensions() {
                return Err(format!(
                    "Qdrant collection dimension {dimensions} does not match configured {}",
                    self.embedding.dimensions()
                ));
            }
        } else if inspect_response.status() == reqwest::StatusCode::NOT_FOUND {
            let mut request = self
                .embedding_http
                .put(format!("{base}/collections/{collection}"))
                .json(&serde_json::json!({"vectors":{"size":self.embedding.dimensions(),"distance":"Cosine"}}));
            if let Some(key) = &self.qdrant_api_key {
                request = request.header("api-key", key);
            }
            let response = request
                .send()
                .await
                .map_err(|e| format!("Qdrant unavailable: {e}"))?;
            if !response.status().is_success() && response.status() != reqwest::StatusCode::CONFLICT
            {
                return Err(format!(
                    "Qdrant collection creation failed: {}",
                    response.status()
                ));
            }
        } else {
            return Err(format!(
                "Qdrant collection inspection failed: {}",
                inspect_response.status()
            ));
        }
        for field in [
            "user_id",
            "embedding_vector_id",
            "content_hash",
            "embedding_model",
            "embedding_version",
            "strategy_family",
        ] {
            let mut request = self
                .embedding_http
                .put(format!("{base}/collections/{collection}/index?wait=true"))
                .json(&serde_json::json!({
                    "field_name": field,
                    "field_schema": "keyword"
                }));
            if let Some(key) = &self.qdrant_api_key {
                request = request.header("api-key", key);
            }
            let response = request
                .send()
                .await
                .map_err(|e| format!("Qdrant payload index creation failed: {e}"))?;
            if !response.status().is_success() && response.status() != reqwest::StatusCode::CONFLICT
            {
                return Err(format!(
                    "Qdrant payload index creation failed for {field}: {}",
                    response.status()
                ));
            }
        }
        self.qdrant_ready_collections
            .write()
            .await
            .insert(collection.to_owned());
        Ok(())
    }

    async fn upsert_canonical_qdrant(
        &self,
        collection: &str,
        user: Uuid,
        canonical_vectors: &[canonical::CanonicalVector],
    ) -> Result<(), String> {
        if canonical_vectors.is_empty() {
            return Ok(());
        }
        let base = self.qdrant_url.trim_end_matches('/');
        let points = canonical_vectors
            .iter()
            .map(|canonical| {
                if canonical.user_id != user || canonical.status != "ready" {
                    metrics::counter!("qdrant_cross_user_attempts_total", "action" => "upsert")
                        .increment(1);
                    return Err(
                        "attempted to upsert a canonical vector outside the authenticated owner"
                            .to_string(),
                    );
                }
                let vector = canonical
                    .embedding
                    .clone()
                    .ok_or_else(|| "canonical vector is missing its embedding".to_string())?;
                Ok(serde_json::json!({
                    "id": canonical.qdrant_point_id,
                    "vector": vector,
                    "payload": {
                        "user_id": user.to_string(),
                        "embedding_vector_id": canonical.id.to_string(),
                        "content_hash": canonical.content_hash,
                        "embedding_model": canonical.embedding_model,
                        "embedding_version": canonical.embedding_version,
                        "strategy_family": canonical.strategy_family
                    }
                }))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut request = self
            .embedding_http
            .put(format!("{base}/collections/{collection}/points?wait=true"))
            .json(&serde_json::json!({"points":points}));
        if let Some(key) = &self.qdrant_api_key {
            request = request.header("api-key", key);
        }
        let response = request
            .send()
            .await
            .map_err(|e| format!("Qdrant upsert failed: {e}"))?;
        if !response.status().is_success() {
            return Err(format!("Qdrant upsert returned {}", response.status()));
        }
        Ok(())
    }

    async fn delete_qdrant_points(
        &self,
        collection: &str,
        user: Uuid,
        file: Uuid,
    ) -> Result<(), String> {
        let base = self.qdrant_url.trim_end_matches('/');
        let mut request = self
            .embedding_http
            .post(format!(
                "{base}/collections/{collection}/points/delete?wait=true"
            ))
            .json(&serde_json::json!({
                "filter": ownership_filter(user, Some(file))
            }));
        if let Some(key) = &self.qdrant_api_key {
            request = request.header("api-key", key);
        }
        let response = request
            .send()
            .await
            .map_err(|e| format!("Qdrant delete failed: {e}"))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(());
        }
        if !response.status().is_success() {
            return Err(format!("Qdrant delete returned {}", response.status()));
        }
        Ok(())
    }

    async fn delete_canonical_qdrant_points(
        &self,
        collection: &str,
        user: Uuid,
        point_ids: &[Uuid],
    ) -> Result<(), String> {
        if point_ids.is_empty() {
            return Ok(());
        }
        let base = self.qdrant_url.trim_end_matches('/');
        let mut request = self
            .embedding_http
            .post(format!(
                "{base}/collections/{collection}/points/delete?wait=true"
            ))
            .json(&serde_json::json!({
                "filter": canonical_ownership_filter(user, point_ids)
            }));
        if let Some(key) = &self.qdrant_api_key {
            request = request.header("api-key", key);
        }
        let response = request
            .send()
            .await
            .map_err(|error| format!("Qdrant canonical delete failed: {error}"))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(());
        }
        if !response.status().is_success() {
            return Err(format!(
                "Qdrant canonical delete returned {}",
                response.status()
            ));
        }
        Ok(())
    }

    async fn flush_pending_canonical_deletions(&self, user: Uuid) -> Result<(), String> {
        let pending = canonical::pending_deletions(&self.pool, user).await?;
        let mut by_collection = HashMap::<String, Vec<canonical::PendingDeletion>>::new();
        for deletion in pending {
            if deletion.user_id != user {
                metrics::counter!("qdrant_cross_user_attempts_total", "action" => "delete")
                    .increment(1);
                return Err("canonical deletion queue returned a foreign owner".into());
            }
            by_collection
                .entry(deletion.collection_name.clone())
                .or_default()
                .push(deletion);
        }
        for (collection, deletions) in by_collection {
            let point_ids = deletions
                .iter()
                .map(|deletion| deletion.qdrant_point_id)
                .collect::<Vec<_>>();
            let vector_ids = deletions
                .iter()
                .map(|deletion| deletion.embedding_vector_id)
                .collect::<Vec<_>>();
            if let Err(error) = self
                .delete_canonical_qdrant_points(&collection, user, &point_ids)
                .await
            {
                canonical::record_deletion_failure(&self.pool, &vector_ids, &error).await?;
                return Err(error);
            }
            canonical::finalize_deletions(&self.pool, &vector_ids).await?;
            metrics::counter!("canonical_embedding_vectors_deleted_total")
                .increment(vector_ids.len() as u64);
        }
        Ok(())
    }

    async fn sync_pending_canonical_vectors(&self, limit: i64) -> Result<usize, String> {
        let pending = canonical::unsynced_vectors(
            &self.pool,
            self.embedding.model(),
            &self.embedding_version,
            limit,
        )
        .await?;
        let pending_count = pending.len();
        let mut groups = HashMap::<(Uuid, String, i32), Vec<canonical::CanonicalVector>>::new();
        for vector in pending {
            let dimension = vector
                .vector_dimension
                .filter(|dimension| *dimension > 0)
                .ok_or_else(|| "canonical vector has no valid dimension".to_string())?;
            if dimension as usize != self.embedding.dimensions() {
                return Err(
                    "canonical vector dimension does not match configured Qwen dimension".into(),
                );
            }
            groups
                .entry((vector.user_id, vector.collection_name.clone(), dimension))
                .or_default()
                .push(vector);
        }
        for ((user, collection, _dimension), vectors) in groups {
            self.ensure_qdrant_collection(&collection).await?;
            self.upsert_canonical_qdrant(&collection, user, &vectors)
                .await?;
            let ids = vectors.iter().map(|vector| vector.id).collect::<Vec<_>>();
            canonical::mark_qdrant_synced(&self.pool, user, &collection, &ids).await?;
        }
        Ok(pending_count)
    }

    async fn delete_all_file_vectors(
        &self,
        user: Uuid,
        file: Uuid,
        reason: &str,
    ) -> Result<i64, String> {
        let stored_collections = sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT collection_name FROM embedding_chunks WHERE user_id=$1 AND file_id=$2",
        )
        .bind(user)
        .bind(file)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        let mut collections = stored_collections.into_iter().collect::<HashSet<_>>();
        collections.insert(self.qdrant_collection.clone());
        collections.insert(format!("{}_{}", self.qdrant_collection, user.simple()));
        for collection in &collections {
            self.delete_qdrant_points(collection, user, file).await?;
        }
        let deleted = canonical::remove_file_references(&self.pool, user, file, reason).await?;
        self.flush_pending_canonical_deletions(user).await?;
        metrics::counter!("file_vectors_deleted_total").increment(deleted.max(0) as u64);
        Ok(deleted)
    }

    async fn search_legacy_vectors(
        &self,
        user: Uuid,
        vector: &[f32],
        limit: i32,
        file_filter: Option<Uuid>,
    ) -> Result<Vec<VectorSearchHit>, Status> {
        let base = self.qdrant_url.trim_end_matches('/');
        let mut request = self
            .embedding_http
            .post(format!(
                "{base}/collections/{}/points/query",
                self.legacy_qdrant_collection
            ))
            .json(&serde_json::json!({
                "query": vector,
                "filter": ownership_filter(user, file_filter),
                "limit": limit,
                "with_payload": true,
                "with_vector": false
            }));
        if let Some(key) = &self.qdrant_api_key {
            request = request.header("api-key", key);
        }
        let response = request.send().await.map_err(|error| {
            Status::unavailable(format!("legacy Qdrant search failed: {error}"))
        })?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        if !response.status().is_success() {
            return Err(Status::unavailable(format!(
                "legacy Qdrant search returned {}",
                response.status()
            )));
        }
        let value: serde_json::Value = response.json().await.map_err(|error| {
            Status::unavailable(format!("invalid legacy Qdrant response: {error}"))
        })?;
        let points = value
            .get("result")
            .and_then(|result| result.get("points").or(Some(result)))
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut scores = HashMap::<Uuid, f32>::new();
        for point in points {
            let Some(payload) = point.get("payload") else {
                metrics::counter!("qdrant_defensive_discards_total", "reason" => "legacy_missing_payload").increment(1);
                continue;
            };
            if !qdrant_payload_belongs_to_user(payload, user) {
                metrics::counter!("qdrant_cross_user_attempts_total", "action" => "legacy_search")
                    .increment(1);
                continue;
            }
            let id = point
                .get("id")
                .and_then(serde_json::Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok())
                .or_else(|| {
                    payload
                        .get("chunk_id")
                        .and_then(serde_json::Value::as_str)
                        .and_then(|value| Uuid::parse_str(value).ok())
                });
            let Some(id) = id else {
                metrics::counter!("qdrant_defensive_discards_total", "reason" => "legacy_invalid_point_id").increment(1);
                continue;
            };
            let score = point
                .get("score")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or_default() as f32;
            scores.insert(id, score);
        }
        if scores.is_empty() {
            return Ok(Vec::new());
        }
        let ids = scores.keys().copied().collect::<Vec<_>>();
        let rows = sqlx::query_as::<_, (Uuid, Uuid, Uuid, Uuid, i32, String, String, String, String)>(
            "SELECT id,file_id,document_id,chunk_id,chunk_index,content,content_hash,embedding_model,embedding_version
             FROM embedding_chunks
             WHERE id=ANY($1) AND user_id=$2 AND ($3::UUID IS NULL OR file_id=$3)
               AND EXISTS(SELECT 1 FROM files WHERE files.id=embedding_chunks.file_id
                         AND files.user_id=embedding_chunks.user_id AND files.status <> 'deleted')",
        )
        .bind(&ids)
        .bind(user)
        .bind(file_filter)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| Status::unavailable(error.to_string()))?;
        let mut hits = rows
            .into_iter()
            .filter_map(|row| {
                Some(VectorSearchHit {
                    chunk_id: row.3.to_string(),
                    document_id: row.2.to_string(),
                    file_id: row.1.to_string(),
                    chunk_index: row.4,
                    content: row.5,
                    content_hash: row.6,
                    embedding_model: row.7,
                    embedding_version: row.8,
                    score: *scores.get(&row.0)?,
                })
            })
            .collect::<Vec<_>>();
        hits.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(limit as usize);
        metrics::counter!("vector_migration_read_v1_total").increment(1);
        Ok(hits)
    }

    async fn perform_vector_search(
        &self,
        user: Uuid,
        query: &str,
        limit: i32,
        file_filter: Option<Uuid>,
    ) -> Result<Vec<VectorSearchHit>, Status> {
        let query = query.trim();
        if query.is_empty() || query.len() > 16_384 {
            return Err(Status::invalid_argument(
                "query must contain between 1 and 16384 characters",
            ));
        }
        if !(1..=100).contains(&limit) {
            return Err(Status::invalid_argument("limit must be between 1 and 100"));
        }
        let limits = self.plan_limits(user).await?;
        if !limits.embeddings_enabled || !limits.rag_enabled || limits.qdrant_mode == "none" {
            return Err(Status::permission_denied(
                "vector search is disabled for the active plan",
            ));
        }
        if let Some(file) = file_filter {
            let owns_file = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM files WHERE id=$1 AND user_id=$2 AND status <> 'deleted')",
            )
            .bind(file)
            .bind(user)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| Status::unavailable(e.to_string()))?;
            if !owns_file {
                return Err(Status::not_found("file not found"));
            }
        }
        let outcome = self
            .embedding
            .embed_query(query)
            .await
            .map_err(Status::unavailable)?;
        let vectors = outcome.vectors;
        let vector = vectors
            .first()
            .ok_or_else(|| Status::unavailable("embedding provider returned no vector"))?;
        if self.vector_read_mode == "v1" {
            return self
                .search_legacy_vectors(user, vector, limit, file_filter)
                .await;
        }
        if self.vector_read_mode == "dual" {
            match self
                .search_legacy_vectors(user, vector, limit, file_filter)
                .await
            {
                Ok(legacy_hits) => {
                    metrics::gauge!("vector_migration_read_v1_results")
                        .set(legacy_hits.len() as f64);
                    metrics::counter!("vector_migration_dual_read_comparisons_total").increment(1);
                }
                Err(error) => {
                    metrics::counter!("vector_migration_dual_read_errors_total").increment(1);
                    tracing::warn!(%error, "legacy v1 read comparison failed; serving v2");
                }
            }
        }
        let collection = self
            .collection_for(user, &limits.qdrant_mode)
            .map_err(Status::failed_precondition)?;
        let restricted_point_ids = if let Some(file) = file_filter {
            let point_ids = canonical::point_ids_for_file(
                &self.pool,
                user,
                file,
                self.embedding_max_file_filter_ids,
            )
            .await
            .map_err(Status::resource_exhausted)?;
            if point_ids.is_empty() {
                return Ok(Vec::new());
            }
            point_ids
        } else {
            Vec::new()
        };
        let filter = canonical_ownership_filter(user, &restricted_point_ids);
        let base = self.qdrant_url.trim_end_matches('/');
        let mut request = self
            .embedding_http
            .post(format!("{base}/collections/{collection}/points/query"))
            .json(&serde_json::json!({
                "query": vector,
                "filter": filter,
                "limit": limit,
                "with_payload": true,
                "with_vector": false
            }));
        if let Some(key) = &self.qdrant_api_key {
            request = request.header("api-key", key);
        }
        let response = request
            .send()
            .await
            .map_err(|e| Status::unavailable(format!("Qdrant search failed: {e}")))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        if !response.status().is_success() {
            return Err(Status::unavailable(format!(
                "Qdrant search returned {}",
                response.status()
            )));
        }
        let value: serde_json::Value = response
            .json()
            .await
            .map_err(|e| Status::unavailable(format!("invalid Qdrant response: {e}")))?;
        let points = value
            .get("result")
            .and_then(|result| result.get("points").or(Some(result)))
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();

        let mut scores = HashMap::<Uuid, f32>::with_capacity(points.len());
        for point in points {
            let Some(payload) = point.get("payload") else {
                metrics::counter!("qdrant_defensive_discards_total", "reason" => "missing_payload")
                    .increment(1);
                continue;
            };
            if !qdrant_payload_belongs_to_user(payload, user) {
                metrics::counter!("qdrant_cross_user_attempts_total", "action" => "search")
                    .increment(1);
                tracing::error!(%user, "Qdrant returned a point outside the mandatory ownership filter");
                continue;
            }
            let required_payload_fields = [
                "embedding_vector_id",
                "content_hash",
                "embedding_model",
                "embedding_version",
                "strategy_family",
            ];
            if required_payload_fields.iter().any(|field| {
                payload
                    .get(*field)
                    .and_then(|value| value.as_str())
                    .is_none()
            }) {
                metrics::counter!("qdrant_defensive_discards_total", "reason" => "incomplete_payload").increment(1);
                tracing::warn!(%user, "Qdrant returned a canonical point with incomplete payload");
                continue;
            }
            let content_hash = payload
                .get("content_hash")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            if content_hash.len() != 64
                || !content_hash
                    .chars()
                    .all(|character| character.is_ascii_hexdigit())
            {
                metrics::counter!("qdrant_defensive_discards_total", "reason" => "invalid_content_hash").increment(1);
                tracing::warn!(%user, "Qdrant returned a canonical point with invalid content hash");
                continue;
            }
            let Some(vector_id) = payload
                .get("embedding_vector_id")
                .and_then(|v| v.as_str())
                .and_then(|v| Uuid::parse_str(v).ok())
            else {
                continue;
            };
            let score = point
                .get("score")
                .and_then(|value| value.as_f64())
                .unwrap_or_default() as f32;
            scores.insert(vector_id, score);
        }
        if scores.is_empty() {
            return Ok(Vec::new());
        }
        let vector_ids = scores.keys().copied().collect::<Vec<_>>();
        let references = canonical::search_references(&self.pool, user, &vector_ids, file_filter)
            .await
            .map_err(Status::unavailable)?;
        let mut hits = references
            .into_iter()
            .filter_map(|reference| {
                let score = scores.get(&reference.embedding_vector_id)?;
                Some(VectorSearchHit {
                    chunk_id: reference.chunk_id.to_string(),
                    document_id: reference.document_id.to_string(),
                    file_id: reference.file_id.to_string(),
                    chunk_index: reference.chunk_index,
                    content: reference.content,
                    content_hash: reference.content_hash,
                    embedding_model: reference.embedding_model,
                    embedding_version: reference.embedding_version,
                    score: *score,
                })
            })
            .collect::<Vec<_>>();
        hits.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.file_id.cmp(&right.file_id))
                .then_with(|| left.chunk_index.cmp(&right.chunk_index))
        });
        hits.truncate(limit as usize);
        metrics::counter!("file_vector_search_total").increment(1);
        Ok(hits)
    }

    async fn verify_uploaded_object(
        &self,
        key: &str,
        expected_size: i64,
        expected_mime: &str,
        expected_sha256: &str,
    ) -> Result<(), Status> {
        let head = self
            .s3
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| Status::failed_precondition(format!("R2 object is not available: {e}")))?;
        if head.content_length().unwrap_or(-1) != expected_size {
            return Err(Status::failed_precondition("uploaded object size mismatch"));
        }
        if let Some(content_type) = head.content_type() {
            if content_type != expected_mime {
                return Err(Status::failed_precondition(
                    "uploaded content-type mismatch",
                ));
            }
        }
        let output = self
            .s3
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| Status::unavailable(format!("R2 object read failed: {e}")))?;
        let mut reader = output.body.into_async_read();
        let mut hasher = Sha256::new();
        let mut sample = Vec::with_capacity(8192);
        let mut buffer = [0u8; 8192];
        let mut total = 0i64;
        loop {
            let read = reader
                .read(&mut buffer)
                .await
                .map_err(|e| Status::unavailable(format!("R2 object stream failed: {e}")))?;
            if read == 0 {
                break;
            }
            total += read as i64;
            hasher.update(&buffer[..read]);
            if sample.len() < 8192 {
                let take = (8192 - sample.len()).min(read);
                sample.extend_from_slice(&buffer[..take]);
            }
        }
        if total != expected_size {
            return Err(Status::failed_precondition("uploaded object size mismatch"));
        }
        let actual_hash = hex::encode(hasher.finalize());
        if actual_hash != expected_sha256 {
            return Err(Status::failed_precondition("uploaded object hash mismatch"));
        }
        if let Some(kind) = infer::get(&sample) {
            let actual_mime = kind.mime_type();
            let office_zip = actual_mime == "application/zip"
                && matches!(expected_mime, "application/zip" | "application/vnd.openxmlformats-officedocument.wordprocessingml.document" | "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" | "application/vnd.openxmlformats-officedocument.presentationml.presentation");
            if actual_mime != expected_mime && !office_zip {
                return Err(Status::failed_precondition("uploaded object MIME mismatch"));
            }
        }
        Ok(())
    }

    async fn plan_limits(
        &self,
        user: Uuid,
    ) -> Result<proto::plan_resolution_v1::GetPlanLimitsResponse, Status> {
        let limits = self.plan_client.limits(user).await?;
        if !limits.has_active_subscription {
            return Err(Status::failed_precondition("active subscription required"));
        }
        Ok(limits)
    }

    fn ingestion_limits(
        limits: &proto::plan_resolution_v1::GetPlanLimitsResponse,
    ) -> quota::IngestionLimits {
        quota::IngestionLimits {
            files_per_hour: limits.max_ingestion_files_per_hour.max(0),
            files_per_day: limits.max_ingestion_files_per_day.max(0),
            bytes_per_day: limits.max_ingestion_bytes_per_day.max(0),
            tokens_per_month: limits.max_ingestion_tokens_per_month.max(0),
            concurrent: limits.max_concurrent_ingestions.max(1),
            pending: limits.max_pending_ingestion_jobs.max(1),
        }
    }

    async fn reserve_with_limits(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        user: Uuid,
        size_bytes: i64,
        limits: &proto::plan_resolution_v1::GetPlanLimitsResponse,
    ) -> Result<(), Status> {
        if limits.max_file_size_bytes > 0 && size_bytes > limits.max_file_size_bytes {
            return Err(Status::resource_exhausted("file size exceeds plan limit"));
        }
        sqlx::query("INSERT INTO file_quotas(user_id,quota_bytes,max_files) VALUES($1,$2,$3) ON CONFLICT(user_id) DO UPDATE SET quota_bytes=EXCLUDED.quota_bytes,max_files=EXCLUDED.max_files,updated_at=now()")
            .bind(user).bind(limits.storage_bytes_quota.max(0)).bind(limits.max_files.max(0))
            .execute(&mut **tx).await.map_err(|e| Status::unavailable(e.to_string()))?;
        let reserved = sqlx::query("UPDATE file_quotas SET used_bytes=used_bytes+$2,file_count=file_count+1,updated_at=now() WHERE user_id=$1 AND (quota_bytes=0 OR used_bytes+$2 <= quota_bytes) AND (max_files=0 OR file_count+1 <= max_files)")
            .bind(user).bind(size_bytes).execute(&mut **tx).await.map_err(|e| Status::unavailable(e.to_string()))?;
        if reserved.rows_affected() != 1 {
            return Err(Status::resource_exhausted("file quota exceeded"));
        }
        Ok(())
    }
}

fn qdrant_collection_name(base: &str, user: Uuid, mode: &str) -> Result<String, String> {
    if !base.ends_with("_v2")
        || base.is_empty()
        || !base
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err("Qdrant collection base must be a safe *_v2 schema name".into());
    }
    match mode {
        "shared" => Ok(base.to_owned()),
        "dedicated" => Ok(format!("{base}_{}", user.simple())),
        _ => Err(format!("unsupported qdrant mode: {mode}")),
    }
}

fn strategy_family(strategy_version: &str) -> String {
    let family = strategy_version
        .split('-')
        .take(3)
        .collect::<Vec<_>>()
        .join("-");
    if family.is_empty() {
        "unknown".to_owned()
    } else {
        family
    }
}

fn ownership_filter(user: Uuid, file: Option<Uuid>) -> serde_json::Value {
    let mut must = vec![serde_json::json!({
        "key": "user_id",
        "match": {"value": user.to_string()}
    })];
    if let Some(file) = file {
        must.push(serde_json::json!({
            "key": "file_id",
            "match": {"value": file.to_string()}
        }));
    }
    serde_json::json!({"must": must})
}

fn canonical_ownership_filter(user: Uuid, point_ids: &[Uuid]) -> serde_json::Value {
    let mut must = vec![serde_json::json!({
        "key": "user_id",
        "match": {"value": user.to_string()}
    })];
    if !point_ids.is_empty() {
        must.push(serde_json::json!({
            "has_id": point_ids.iter().map(Uuid::to_string).collect::<Vec<_>>()
        }));
    }
    serde_json::json!({"must": must})
}

fn extract_document_text(mime: &str, bytes: &[u8]) -> Result<String, String> {
    if mime == "text/plain" || mime == "text/csv" {
        return String::from_utf8(bytes.to_vec())
            .map_err(|_| "text file is not valid UTF-8".into());
    }
    if mime == "application/pdf" {
        return pdf_extract::extract_text_from_mem(bytes)
            .map_err(|e| format!("PDF text extraction failed: {e}"));
    }
    if mime.contains("wordprocessingml.document") || mime.contains("spreadsheetml.sheet") {
        let cursor = Cursor::new(bytes);
        let mut archive =
            zip::ZipArchive::new(cursor).map_err(|e| format!("invalid Office document: {e}"))?;
        let names = if mime.contains("wordprocessing") {
            vec!["word/document.xml"]
        } else {
            vec!["xl/sharedStrings.xml", "xl/worksheets/sheet1.xml"]
        };
        let mut output = String::new();
        for name in names {
            if let Ok(mut file) = archive.by_name(name) {
                let mut xml = String::new();
                file.read_to_string(&mut xml).map_err(|e| e.to_string())?;
                output.push_str(&strip_xml(&xml));
                output.push('\n');
            }
        }
        return Ok(output);
    }
    Err(format!("unsupported document MIME type: {mime}"))
}

fn strip_xml(xml: &str) -> String {
    let mut text = String::with_capacity(xml.len());
    let mut in_tag = false;
    for ch in xml.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(ch),
            _ => {}
        }
    }
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

#[derive(Clone)]
struct PlanClient {
    endpoint: String,
    secret: String,
    issuer: String,
    subject: String,
    tls: ServiceTlsConfig,
}

impl PlanClient {
    async fn channel(&self) -> Result<Channel, Status> {
        let endpoint = Endpoint::from_shared(self.endpoint.clone())
            .map_err(|e| Status::unavailable(e.to_string()))?;
        let endpoint = if self.tls.is_enabled() {
            common::tls::configure_client_endpoint(endpoint, &self.endpoint, &self.tls)
                .await
                .map_err(|e| Status::unavailable(e.to_string()))?
        } else {
            endpoint
        };
        endpoint
            .connect()
            .await
            .map_err(|e| Status::unavailable(e.to_string()))
    }

    async fn limits(
        &self,
        user_id: Uuid,
    ) -> Result<proto::plan_resolution_v1::GetPlanLimitsResponse, Status> {
        let claims = ServiceJwtClaims::new(&self.issuer, &self.subject, 60)
            .map_err(|e| Status::internal(e.to_string()))?;
        let token = claims
            .sign_with_kid(&derive_kid(self.secret.as_bytes()), self.secret.as_bytes())
            .map_err(|e| Status::internal(e.to_string()))?;
        let mut request = Request::new(GetPlanLimitsRequest {
            user_id: user_id.to_string(),
        });
        request.metadata_mut().insert(
            "authorization",
            MetadataValue::try_from(format!("Bearer {token}"))
                .map_err(|e| Status::internal(e.to_string()))?,
        );
        PlanResolutionServiceClient::new(self.channel().await?)
            .get_plan_limits(request)
            .await
            .map(|response| response.into_inner())
            .map_err(|e| Status::unavailable(e.to_string()))
    }
}
fn uid(v: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(v).map_err(|_| Status::invalid_argument("invalid user_id"))
}
fn optional_uid(v: Option<&str>) -> Result<Option<Uuid>, Status> {
    v.map(|value| Uuid::parse_str(value).map_err(|_| Status::invalid_argument("invalid folder_id")))
        .transpose()
}

fn qdrant_payload_belongs_to_user(payload: &serde_json::Value, user: Uuid) -> bool {
    payload
        .get("user_id")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|owner| owner == user.to_string())
}

fn require_vector_caller<T>(request: &Request<T>) -> Result<(), Status> {
    let caller = request
        .extensions()
        .get::<ServiceJwtClaims>()
        .map(|claims| claims.sub.as_str())
        .unwrap_or_default();
    if matches!(caller, "customer-bff" | "admin-bff") {
        Ok(())
    } else {
        Err(Status::permission_denied(
            "vector operations require an authorized BFF",
        ))
    }
}
type FileRow = (
    Uuid,
    Uuid,
    Option<Uuid>,
    String,
    String,
    i64,
    String,
    String,
    String,
    chrono::DateTime<chrono::Utc>,
    chrono::DateTime<chrono::Utc>,
);

fn map(row: FileRow) -> File {
    File {
        id: row.0.to_string(),
        user_id: row.1.to_string(),
        folder_id: row.2.map(|v| v.to_string()),
        name: row.3,
        mime_type: row.4,
        size_bytes: row.5,
        sha256: row.6,
        status: row.7,
        object_key: row.8,
        created_at: row.9.to_rfc3339(),
        updated_at: row.10.to_rfc3339(),
    }
}
type FolderRow = (
    Uuid,
    Uuid,
    Option<Uuid>,
    String,
    chrono::DateTime<chrono::Utc>,
    chrono::DateTime<chrono::Utc>,
);

fn map_folder(row: FolderRow) -> Folder {
    Folder {
        id: row.0.to_string(),
        user_id: row.1.to_string(),
        parent_id: row.2.map(|v| v.to_string()),
        name: row.3,
        created_at: row.4.to_rfc3339(),
        updated_at: row.5.to_rfc3339(),
    }
}
#[tonic::async_trait]
impl FileService for App {
    async fn create_folder(
        &self,
        r: Request<CreateFolderRequest>,
    ) -> Result<Response<FolderResponse>, Status> {
        let r = r.into_inner();
        let user = uid(&r.user_id)?;
        self.plan_limits(user).await?;
        let name = r.name.trim();
        if name.is_empty()
            || name.len() > 255
            || r.idempotency_key.trim().is_empty()
            || r.idempotency_key.len() > 255
        {
            return Err(Status::invalid_argument("invalid folder metadata"));
        }
        let parent_id = optional_uid(r.parent_id.as_deref())?;
        if let Some(parent_id) = parent_id {
            let owns_parent = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM folders WHERE id=$1 AND user_id=$2 AND status='active')",
            )
            .bind(parent_id)
            .bind(user)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| Status::unavailable(e.to_string()))?;
            if !owns_parent {
                return Err(Status::not_found("parent folder not found"));
            }
        }
        let row = sqlx::query_as::<_, (Uuid, Uuid, Option<Uuid>, String, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)>(
            "INSERT INTO folders(id,user_id,parent_id,name,idempotency_key) VALUES($1,$2,$3,$4,$5) ON CONFLICT(user_id,idempotency_key) DO UPDATE SET updated_at=folders.updated_at RETURNING id,user_id,parent_id,name,created_at,updated_at",
        )
        .bind(Uuid::now_v7()).bind(user).bind(parent_id).bind(name).bind(r.idempotency_key.trim())
        .fetch_one(&self.pool).await.map_err(|e| Status::already_exists(format!("folder already exists: {e}")))?;
        sqlx::query("INSERT INTO file_audit_log(id,user_id,action,metadata) VALUES($1,$2,'folder_created',$3)")
            .bind(Uuid::now_v7()).bind(user).bind(serde_json::json!({"folder_id": row.0, "name": name}))
            .execute(&self.pool).await.map_err(|e| Status::unavailable(e.to_string()))?;
        Ok(Response::new(FolderResponse {
            folder: Some(map_folder(row)),
        }))
    }

    async fn list_folders(
        &self,
        r: Request<ListFoldersRequest>,
    ) -> Result<Response<ListFoldersResponse>, Status> {
        let r = r.into_inner();
        let user = uid(&r.user_id)?;
        self.plan_limits(user).await?;
        let parent_id = optional_uid(r.parent_id.as_deref())?;
        let rows = sqlx::query_as::<_, (Uuid, Uuid, Option<Uuid>, String, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)>(
            "SELECT id,user_id,parent_id,name,created_at,updated_at FROM folders WHERE user_id=$1 AND status='active' AND ($2::uuid IS NULL OR parent_id=$2) ORDER BY name",
        ).bind(user).bind(parent_id).fetch_all(&self.pool).await.map_err(|e| Status::unavailable(e.to_string()))?;
        sqlx::query("INSERT INTO file_audit_log(id,user_id,action,metadata) VALUES($1,$2,'folders_listed',$3)")
            .bind(Uuid::now_v7())
            .bind(user)
            .bind(serde_json::json!({"parent_id": parent_id, "result_count": rows.len()}))
            .execute(&self.pool)
            .await
            .map_err(|e| Status::unavailable(e.to_string()))?;
        metrics::counter!("file_audit_events_total", "action" => "folders_listed", "user_hash" => common::observability::hash_user_id(user)).increment(1);
        Ok(Response::new(ListFoldersResponse {
            folders: rows.into_iter().map(map_folder).collect(),
        }))
    }

    async fn create_file(
        &self,
        r: Request<CreateFileRequest>,
    ) -> Result<Response<FileResponse>, Status> {
        let r = r.into_inner();
        let user = uid(&r.user_id)?;
        let limits = self.plan_limits(user).await?;
        let name = r.name.trim();
        if name.is_empty()
            || name.len() > 255
            || r.idempotency_key.trim().is_empty()
            || r.idempotency_key.len() > 255
            || r.mime_type.trim().is_empty()
            || r.mime_type.len() > 255
            || r.sha256.len() != 64
            || !r.sha256.chars().all(|c| c.is_ascii_hexdigit())
            || r.size_bytes < 0
            || r.size_bytes > self.max_file_bytes
        {
            return Err(Status::invalid_argument(
                "invalid file metadata or size limit",
            ));
        }
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| Status::unavailable(e.to_string()))?;
        let existing=sqlx::query_as::<_,(Uuid,Uuid,Option<Uuid>,String,String,i64,String,String,String,chrono::DateTime<chrono::Utc>,chrono::DateTime<chrono::Utc>)>("SELECT id,user_id,folder_id,name,mime_type,size_bytes,sha256,status,object_key,created_at,updated_at FROM files WHERE user_id=$1 AND idempotency_key=$2").bind(user).bind(&r.idempotency_key).fetch_optional(&mut *tx).await.map_err(|e|Status::unavailable(e.to_string()))?;
        if let Some(row) = existing {
            tx.rollback().await.ok();
            return Ok(Response::new(FileResponse {
                file: Some(map(row)),
            }));
        }
        self.reserve_with_limits(&mut tx, user, r.size_bytes, &limits)
            .await?;
        let folder_id = optional_uid(r.folder_id.as_deref())?;
        if let Some(folder_id) = folder_id {
            let owns_folder = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM folders WHERE id=$1 AND user_id=$2)",
            )
            .bind(folder_id)
            .bind(user)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| Status::unavailable(e.to_string()))?;
            if !owns_folder {
                return Err(Status::not_found("folder not found"));
            }
        }
        let id = Uuid::now_v7();
        // Normal uploads use opaque keys. The S3 gateway may supply a scoped
        // key so S3 object names can remain stable; it is strictly namespaced
        // by the authenticated user and cannot escape the R2 prefix.
        let key = if let Some(requested) = r.object_key.as_deref() {
            let requested = requested.trim();
            if requested.is_empty()
                || requested.len() > 1024
                || requested.contains("..")
                || requested.starts_with('/')
                || !requested.starts_with(&format!("s3/{user}/"))
            {
                return Err(Status::invalid_argument("invalid object key"));
            }
            requested.to_owned()
        } else {
            format!("files/{user}/{id}")
        };
        let row=sqlx::query_as::<_,(Uuid,Uuid,Option<Uuid>,String,String,i64,String,String,String,chrono::DateTime<chrono::Utc>,chrono::DateTime<chrono::Utc>)>("INSERT INTO files(id,user_id,folder_id,name,mime_type,size_bytes,sha256,status,object_key,idempotency_key) VALUES($1,$2,$3,$4,$5,$6,$7,'pending',$8,$9) RETURNING id,user_id,folder_id,name,mime_type,size_bytes,sha256,status,object_key,created_at,updated_at").bind(id).bind(user).bind(folder_id).bind(name).bind(r.mime_type.trim()).bind(r.size_bytes).bind(r.sha256.to_ascii_lowercase()).bind(&key).bind(r.idempotency_key.trim()).fetch_one(&mut *tx).await.map_err(|e|Status::unavailable(e.to_string()))?;
        sqlx::query("INSERT INTO file_audit_log(id,user_id,file_id,action,metadata) VALUES($1,$2,$3,'created',$4)").bind(Uuid::now_v7()).bind(user).bind(id).bind(serde_json::json!({"status":"pending"})).execute(&mut *tx).await.map_err(|e|Status::unavailable(e.to_string()))?;
        tx.commit()
            .await
            .map_err(|e| Status::unavailable(e.to_string()))?;
        Ok(Response::new(FileResponse {
            file: Some(map(row.clone())),
        }))
    }
    async fn list_files(
        &self,
        r: Request<ListFilesRequest>,
    ) -> Result<Response<ListFilesResponse>, Status> {
        let r = r.into_inner();
        let user = uid(&r.user_id)?;
        self.plan_limits(user).await?;
        let folder_id = optional_uid(r.folder_id.as_deref())?;
        let rows=sqlx::query_as::<_,(Uuid,Uuid,Option<Uuid>,String,String,i64,String,String,String,chrono::DateTime<chrono::Utc>,chrono::DateTime<chrono::Utc>)>("SELECT id,user_id,folder_id,name,mime_type,size_bytes,sha256,status,object_key,created_at,updated_at FROM files WHERE user_id=$1 AND status<>'deleted' AND ($2::uuid IS NULL OR folder_id=$2) ORDER BY created_at DESC").bind(user).bind(folder_id).fetch_all(&self.pool).await.map_err(|e|Status::unavailable(e.to_string()))?;
        sqlx::query("INSERT INTO file_audit_log(id,user_id,action,metadata) VALUES($1,$2,'files_listed',$3)")
            .bind(Uuid::now_v7())
            .bind(user)
            .bind(serde_json::json!({"folder_id": folder_id, "result_count": rows.len()}))
            .execute(&self.pool)
            .await
            .map_err(|e| Status::unavailable(e.to_string()))?;
        metrics::counter!("file_audit_events_total", "action" => "files_listed", "user_hash" => common::observability::hash_user_id(user)).increment(1);
        Ok(Response::new(ListFilesResponse {
            files: rows.into_iter().map(map).collect(),
        }))
    }
    async fn confirm_file(
        &self,
        r: Request<ConfirmFileRequest>,
    ) -> Result<Response<FileResponse>, Status> {
        let r = r.into_inner();
        let user = uid(&r.user_id)?;
        let limits = self.plan_limits(user).await?;
        let id =
            Uuid::parse_str(&r.file_id).map_err(|_| Status::invalid_argument("invalid file_id"))?;
        let pending = sqlx::query_as::<_, (Uuid, Uuid, Option<Uuid>, String, String, i64, String, String, String, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)>(
            "SELECT id,user_id,folder_id,name,mime_type,size_bytes,sha256,status,object_key,created_at,updated_at FROM files WHERE id=$1 AND user_id=$2 AND status='pending'",
        ).bind(id).bind(user).fetch_optional(&self.pool).await
            .map_err(|e| Status::unavailable(e.to_string()))?
            .ok_or_else(|| Status::not_found("file not found or already confirmed"))?;
        let reservation_id = self
            .quota
            .reserve(user, id, pending.5, Self::ingestion_limits(&limits))
            .await
            .map_err(Status::unavailable)?;
        let Some(reservation_id) = reservation_id else {
            metrics::counter!("file_ingestion_quota_rejections_total", "reason" => "plan_limit", "user_hash" => common::observability::hash_user_id(user))
                .increment(1);
            return Err(Status::resource_exhausted("ingestion quota exceeded"));
        };
        if let Err(error) = self
            .verify_uploaded_object(&pending.8, pending.5, &pending.4, &pending.6)
            .await
        {
            let _ = self
                .quota
                .release_reservation(user, reservation_id, &error.to_string())
                .await;
            sqlx::query(
                "UPDATE files SET status='failed',updated_at=now() WHERE id=$1 AND user_id=$2",
            )
            .bind(id)
            .bind(user)
            .execute(&self.pool)
            .await
            .map_err(|e| Status::unavailable(e.to_string()))?;
            sqlx::query("INSERT INTO file_audit_log(id,user_id,file_id,action,metadata) VALUES($1,$2,$3,'upload_failed',$4)")
                .bind(Uuid::now_v7()).bind(user).bind(id)
                .bind(serde_json::json!({"error": error.to_string()})).execute(&self.pool).await
                .map_err(|e| Status::unavailable(e.to_string()))?;
            return Err(error);
        }
        let mut tx = match self.pool.begin().await {
            Ok(tx) => tx,
            Err(error) => {
                let _ = self
                    .quota
                    .release_reservation(user, reservation_id, &error.to_string())
                    .await;
                return Err(Status::unavailable(error.to_string()));
            }
        };
        let row=match sqlx::query_as::<_,(Uuid,Uuid,Option<Uuid>,String,String,i64,String,String,String,chrono::DateTime<chrono::Utc>,chrono::DateTime<chrono::Utc>)>("UPDATE files SET status='ready',updated_at=now() WHERE id=$1 AND user_id=$2 AND status='pending' RETURNING id,user_id,folder_id,name,mime_type,size_bytes,sha256,status,object_key,created_at,updated_at").bind(id).bind(user).fetch_optional(&mut *tx).await {
            Ok(Some(row)) => row,
            Ok(None) => {
                let _ = tx.rollback().await;
                let _ = self.quota.release_reservation(user, reservation_id, "file was already confirmed").await;
                return Err(Status::not_found("file not found or already confirmed"));
            }
            Err(error) => {
                let _ = tx.rollback().await;
                let _ = self.quota.release_reservation(user, reservation_id, &error.to_string()).await;
                return Err(Status::unavailable(error.to_string()));
            }
        };
        if let Err(error) = sqlx::query("INSERT INTO file_audit_log(id,user_id,file_id,action,metadata) VALUES($1,$2,$3,'confirmed',$4)")
            .bind(Uuid::now_v7()).bind(user).bind(id)
            .bind(serde_json::json!({"status":"ready"}))
            .execute(&mut *tx).await
        {
            let _ = tx.rollback().await;
            let _ = self.quota.release_reservation(user, reservation_id, &error.to_string()).await;
            return Err(Status::unavailable(error.to_string()));
        }
        let ingest = EventEnvelope::new(
            EVENT_FILE_INGEST_REQUESTED,
            FileIngestRequested {
                file_id: row.0,
                user_id: row.1,
                object_key: row.8.clone(),
                mime_type: row.4.clone(),
                size_bytes: row.5,
                strategy_version: self.ingestion_strategy_version.clone(),
                embedding_model: self.embedding_model.clone(),
                quota_reservation_id: Some(reservation_id),
            },
        );
        if let Err(error) = enqueue_in_transaction(
            &mut tx,
            "event_outbox",
            EVENT_FILE_INGEST_REQUESTED,
            &ingest,
        )
        .await
        {
            let _ = tx.rollback().await;
            let _ = self
                .quota
                .release_reservation(user, reservation_id, &error.to_string())
                .await;
            return Err(Status::unavailable(format!(
                "failed to enqueue ingestion: {error}"
            )));
        }
        if let Err(error) = tx.commit().await {
            let _ = self
                .quota
                .release_reservation(user, reservation_id, &error.to_string())
                .await;
            return Err(Status::unavailable(error.to_string()));
        }
        Ok(Response::new(FileResponse {
            file: Some(map(row)),
        }))
    }
    async fn delete_file(
        &self,
        r: Request<DeleteFileRequest>,
    ) -> Result<Response<DeleteFileResponse>, Status> {
        let r = r.into_inner();
        let user = uid(&r.user_id)?;
        self.plan_limits(user).await?;
        let id =
            Uuid::parse_str(&r.file_id).map_err(|_| Status::invalid_argument("invalid file_id"))?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| Status::unavailable(e.to_string()))?;
        let size=sqlx::query_scalar::<_,i64>("UPDATE files SET status='deleted',updated_at=now() WHERE id=$1 AND user_id=$2 AND status<>'deleted' RETURNING size_bytes").bind(id).bind(user).fetch_optional(&mut *tx).await.map_err(|e|Status::unavailable(e.to_string()))?.ok_or_else(||Status::not_found("file not found"))?;
        sqlx::query("UPDATE file_quotas SET used_bytes=GREATEST(0,used_bytes-$2),file_count=GREATEST(0,file_count-1),updated_at=now() WHERE user_id=$1").bind(user).bind(size).execute(&mut *tx).await.map_err(|e|Status::unavailable(e.to_string()))?;
        sqlx::query("INSERT INTO file_audit_log(id,user_id,file_id,action,metadata) VALUES($1,$2,$3,'deleted',$4)").bind(Uuid::now_v7()).bind(user).bind(id).bind(serde_json::json!({"reason":r.reason})).execute(&mut *tx).await.map_err(|e|Status::unavailable(e.to_string()))?;
        let delete_vectors = EventEnvelope::new(
            EVENT_FILE_VECTORS_DELETE_REQUESTED,
            FileVectorsDeleteRequested {
                file_id: id,
                user_id: user,
                reason: r.reason.clone(),
            },
        );
        enqueue_in_transaction(
            &mut tx,
            "event_outbox",
            EVENT_FILE_VECTORS_DELETE_REQUESTED,
            &delete_vectors,
        )
        .await
        .map_err(|e| Status::unavailable(format!("failed to enqueue vector deletion: {e}")))?;
        tx.commit()
            .await
            .map_err(|e| Status::unavailable(e.to_string()))?;
        Ok(Response::new(DeleteFileResponse { success: true }))
    }

    async fn update_file_status(
        &self,
        r: Request<UpdateFileStatusRequest>,
    ) -> Result<Response<FileResponse>, Status> {
        let r = r.into_inner();
        let user = uid(&r.user_id)?;
        self.plan_limits(user).await?;
        let id =
            Uuid::parse_str(&r.file_id).map_err(|_| Status::invalid_argument("invalid file_id"))?;
        if !matches!(r.status.as_str(), "processing" | "ready" | "failed") {
            return Err(Status::invalid_argument("unsupported file status"));
        }
        let row = sqlx::query_as::<_, (Uuid, Uuid, Option<Uuid>, String, String, i64, String, String, String, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)>(
            "UPDATE files SET status=$3,updated_at=now() WHERE id=$1 AND user_id=$2 AND status <> 'deleted' RETURNING id,user_id,folder_id,name,mime_type,size_bytes,sha256,status,object_key,created_at,updated_at",
        ).bind(id).bind(user).bind(&r.status).fetch_optional(&self.pool).await
            .map_err(|e| Status::unavailable(e.to_string()))?
            .ok_or_else(|| Status::not_found("file not found"))?;
        sqlx::query("INSERT INTO file_audit_log(id,user_id,file_id,action,metadata) VALUES($1,$2,$3,'status_changed',$4)")
            .bind(Uuid::now_v7()).bind(user).bind(id)
            .bind(serde_json::json!({"status": r.status, "error": r.error}))
            .execute(&self.pool).await.map_err(|e| Status::unavailable(e.to_string()))?;
        Ok(Response::new(FileResponse {
            file: Some(map(row)),
        }))
    }

    async fn create_upload_url(
        &self,
        r: Request<CreateUploadUrlRequest>,
    ) -> Result<Response<UploadUrlResponse>, Status> {
        let r = r.into_inner();
        let user = uid(&r.user_id)?;
        self.plan_limits(user).await?;
        let id =
            Uuid::parse_str(&r.file_id).map_err(|_| Status::invalid_argument("invalid file_id"))?;
        let row = sqlx::query_as::<_, (Uuid, Uuid, Option<Uuid>, String, String, i64, String, String, String, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)>(
            "SELECT id,user_id,folder_id,name,mime_type,size_bytes,sha256,status,object_key,created_at,updated_at FROM files WHERE id=$1 AND user_id=$2",
        ).bind(id).bind(user).fetch_optional(&self.pool).await
            .map_err(|e| Status::unavailable(e.to_string()))?
            .ok_or_else(|| Status::not_found("file not found"))?;
        if row.7 != "pending" {
            return Err(Status::failed_precondition("file is not pending upload"));
        }
        let ttl = r
            .expires_seconds
            .unwrap_or(self.upload_url_ttl_seconds as i32)
            .clamp(60, 900) as u64;
        let presigned = self
            .s3
            .put_object()
            .bucket(&self.bucket)
            .key(&row.8)
            .content_type(&row.4)
            .metadata("sha256", &row.6)
            .presigned(
                PresigningConfig::expires_in(Duration::from_secs(ttl))
                    .map_err(|e| Status::internal(e.to_string()))?,
            )
            .await
            .map_err(|e| Status::unavailable(e.to_string()))?;
        let expires_at = chrono::Utc::now() + chrono::Duration::seconds(ttl as i64);
        sqlx::query("INSERT INTO file_audit_log(id,user_id,file_id,action,metadata) VALUES($1,$2,$3,'upload_url_issued',$4)")
            .bind(Uuid::now_v7())
            .bind(user)
            .bind(id)
            .bind(serde_json::json!({"expires_at": expires_at, "ttl_seconds": ttl}))
            .execute(&self.pool)
            .await
            .map_err(|e| Status::unavailable(e.to_string()))?;
        metrics::counter!("file_audit_events_total", "action" => "upload_url_issued", "user_hash" => common::observability::hash_user_id(user)).increment(1);
        Ok(Response::new(UploadUrlResponse {
            file: Some(map(row.clone())),
            upload_url: presigned.uri().to_string(),
            required_headers: [
                ("content-type".to_string(), row.4),
                ("x-amz-meta-sha256".to_string(), row.6),
            ]
            .into_iter()
            .collect(),
            expires_at: expires_at.to_rfc3339(),
        }))
    }

    async fn purge_file(
        &self,
        request: Request<PurgeFileRequest>,
    ) -> Result<Response<PurgeFileResponse>, Status> {
        let caller = request
            .extensions()
            .get::<ServiceJwtClaims>()
            .map(|claims| claims.sub.as_str())
            .unwrap_or_default();
        if caller != "admin-bff" {
            return Err(Status::permission_denied(
                "physical deletion requires admin authorization",
            ));
        }
        let r = request.into_inner();
        let user = uid(&r.user_id)?;
        let id =
            Uuid::parse_str(&r.file_id).map_err(|_| Status::invalid_argument("invalid file_id"))?;
        let row = sqlx::query_as::<_, (String, String)>(
            "SELECT object_key,status FROM files WHERE id=$1 AND user_id=$2",
        )
        .bind(id)
        .bind(user)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Status::unavailable(e.to_string()))?
        .ok_or_else(|| Status::not_found("file not found"))?;
        if row.1 != "deleted" {
            return Err(Status::failed_precondition(
                "file must be logically deleted first",
            ));
        }
        self.s3
            .delete_object()
            .bucket(&self.bucket)
            .key(&row.0)
            .send()
            .await
            .map_err(|e| Status::unavailable(format!("R2 delete failed: {e}")))?;
        sqlx::query("INSERT INTO file_audit_log(id,user_id,file_id,action,metadata) VALUES($1,$2,$3,'purged',$4)")
            .bind(Uuid::now_v7()).bind(user).bind(id)
            .bind(serde_json::json!({"reason": r.reason, "object_key": row.0}))
            .execute(&self.pool).await.map_err(|e| Status::unavailable(e.to_string()))?;
        Ok(Response::new(PurgeFileResponse { success: true }))
    }

    async fn create_download_url(
        &self,
        r: Request<CreateDownloadUrlRequest>,
    ) -> Result<Response<DownloadUrlResponse>, Status> {
        let r = r.into_inner();
        let user = uid(&r.user_id)?;
        self.plan_limits(user).await?;
        let id =
            Uuid::parse_str(&r.file_id).map_err(|_| Status::invalid_argument("invalid file_id"))?;
        let row = sqlx::query_as::<_, (Uuid, Uuid, Option<Uuid>, String, String, i64, String, String, String, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)>("SELECT id,user_id,folder_id,name,mime_type,size_bytes,sha256,status,object_key,created_at,updated_at FROM files WHERE id=$1 AND user_id=$2 AND status='ready'").bind(id).bind(user).fetch_optional(&self.pool).await.map_err(|e| Status::unavailable(e.to_string()))?.ok_or_else(|| Status::not_found("ready file not found"))?;
        let ttl = r
            .expires_seconds
            .unwrap_or(self.upload_url_ttl_seconds as i32)
            .clamp(60, 900) as u64;
        let signed = self
            .s3
            .get_object()
            .bucket(&self.bucket)
            .key(&row.8)
            .presigned(
                PresigningConfig::expires_in(Duration::from_secs(ttl))
                    .map_err(|e| Status::internal(e.to_string()))?,
            )
            .await
            .map_err(|e| Status::unavailable(e.to_string()))?;
        sqlx::query("INSERT INTO file_audit_log(id,user_id,file_id,action,metadata) VALUES($1,$2,$3,'download_url_issued',$4)")
            .bind(Uuid::now_v7())
            .bind(user)
            .bind(id)
            .bind(serde_json::json!({"expires_seconds": ttl}))
            .execute(&self.pool)
            .await
            .map_err(|e| Status::unavailable(e.to_string()))?;
        metrics::counter!("file_audit_events_total", "action" => "download_url_issued", "user_hash" => common::observability::hash_user_id(user)).increment(1);
        Ok(Response::new(DownloadUrlResponse {
            file: Some(map(row)),
            download_url: signed.uri().to_string(),
            expires_at: (chrono::Utc::now() + chrono::Duration::seconds(ttl as i64)).to_rfc3339(),
        }))
    }

    async fn update_file(
        &self,
        r: Request<UpdateFileRequest>,
    ) -> Result<Response<FileResponse>, Status> {
        let r = r.into_inner();
        let user = uid(&r.user_id)?;
        self.plan_limits(user).await?;
        let id =
            Uuid::parse_str(&r.file_id).map_err(|_| Status::invalid_argument("invalid file_id"))?;
        if r.name
            .as_deref()
            .is_some_and(|name| name.trim().is_empty() || name.len() > 255)
        {
            return Err(Status::invalid_argument("invalid file name"));
        }
        let folder_id = optional_uid(r.folder_id.as_deref())?;
        if let Some(folder_id) = folder_id {
            let ok = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM folders WHERE id=$1 AND user_id=$2 AND status='active')").bind(folder_id).bind(user).fetch_one(&self.pool).await.map_err(|e| Status::unavailable(e.to_string()))?;
            if !ok {
                return Err(Status::not_found("folder not found"));
            }
        }
        let row = sqlx::query_as::<_, (Uuid, Uuid, Option<Uuid>, String, String, i64, String, String, String, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)>("UPDATE files SET name=COALESCE($3,name),folder_id=COALESCE($4,folder_id),updated_at=now() WHERE id=$1 AND user_id=$2 AND status <> 'deleted' RETURNING id,user_id,folder_id,name,mime_type,size_bytes,sha256,status,object_key,created_at,updated_at").bind(id).bind(user).bind(r.name.as_deref().map(str::trim)).bind(folder_id).fetch_optional(&self.pool).await.map_err(|e| Status::unavailable(e.to_string()))?.ok_or_else(|| Status::not_found("file not found"))?;
        sqlx::query("INSERT INTO file_audit_log(id,user_id,file_id,action,metadata) VALUES($1,$2,$3,'file_updated',$4)").bind(Uuid::now_v7()).bind(user).bind(id).bind(serde_json::json!({"name": r.name, "folder_id": r.folder_id})).execute(&self.pool).await.map_err(|e| Status::unavailable(e.to_string()))?;
        Ok(Response::new(FileResponse {
            file: Some(map(row)),
        }))
    }

    async fn update_folder(
        &self,
        r: Request<UpdateFolderRequest>,
    ) -> Result<Response<FolderResponse>, Status> {
        let r = r.into_inner();
        let user = uid(&r.user_id)?;
        self.plan_limits(user).await?;
        let id = Uuid::parse_str(&r.folder_id)
            .map_err(|_| Status::invalid_argument("invalid folder_id"))?;
        if r.name
            .as_deref()
            .is_some_and(|name| name.trim().is_empty() || name.len() > 255)
        {
            return Err(Status::invalid_argument("invalid folder name"));
        }
        let parent_id = optional_uid(r.parent_id.as_deref())?;
        if let Some(parent_id) = parent_id {
            if parent_id == id {
                return Err(Status::invalid_argument("folder cannot contain itself"));
            }
            let ok = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM folders WHERE id=$1 AND user_id=$2 AND status='active')").bind(parent_id).bind(user).fetch_one(&self.pool).await.map_err(|e| Status::unavailable(e.to_string()))?;
            if !ok {
                return Err(Status::not_found("parent folder not found"));
            }
        }
        let row = sqlx::query_as::<_, (Uuid, Uuid, Option<Uuid>, String, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)>("UPDATE folders SET name=COALESCE($3,name),parent_id=COALESCE($4,parent_id),updated_at=now() WHERE id=$1 AND user_id=$2 AND status='active' RETURNING id,user_id,parent_id,name,created_at,updated_at").bind(id).bind(user).bind(r.name.as_deref().map(str::trim)).bind(parent_id).fetch_optional(&self.pool).await.map_err(|e| Status::unavailable(e.to_string()))?.ok_or_else(|| Status::not_found("folder not found"))?;
        sqlx::query("INSERT INTO file_audit_log(id,user_id,action,metadata) VALUES($1,$2,'folder_updated',$3)").bind(Uuid::now_v7()).bind(user).bind(serde_json::json!({"folder_id": id, "name": r.name, "parent_id": r.parent_id})).execute(&self.pool).await.map_err(|e| Status::unavailable(e.to_string()))?;
        Ok(Response::new(FolderResponse {
            folder: Some(map_folder(row)),
        }))
    }

    async fn delete_folder(
        &self,
        r: Request<DeleteFolderRequest>,
    ) -> Result<Response<DeleteFolderResponse>, Status> {
        let r = r.into_inner();
        let user = uid(&r.user_id)?;
        self.plan_limits(user).await?;
        let id = Uuid::parse_str(&r.folder_id)
            .map_err(|_| Status::invalid_argument("invalid folder_id"))?;
        let blocked = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM files WHERE folder_id=$1 AND user_id=$2 AND status <> 'deleted') OR EXISTS(SELECT 1 FROM folders WHERE parent_id=$1 AND user_id=$2 AND status='active')").bind(id).bind(user).fetch_one(&self.pool).await.map_err(|e| Status::unavailable(e.to_string()))?;
        if blocked {
            return Err(Status::failed_precondition(
                "folder must be empty before deletion",
            ));
        }
        let changed = sqlx::query("UPDATE folders SET status='deleted',updated_at=now() WHERE id=$1 AND user_id=$2 AND status='active'").bind(id).bind(user).execute(&self.pool).await.map_err(|e| Status::unavailable(e.to_string()))?;
        if changed.rows_affected() != 1 {
            return Err(Status::not_found("folder not found"));
        }
        sqlx::query("INSERT INTO file_audit_log(id,user_id,action,metadata) VALUES($1,$2,'folder_deleted',$3)").bind(Uuid::now_v7()).bind(user).bind(serde_json::json!({"folder_id": id, "reason": r.reason})).execute(&self.pool).await.map_err(|e| Status::unavailable(e.to_string()))?;
        Ok(Response::new(DeleteFolderResponse { success: true }))
    }

    async fn search_vectors(
        &self,
        r: Request<SearchVectorsRequest>,
    ) -> Result<Response<SearchVectorsResponse>, Status> {
        require_vector_caller(&r)?;
        let r = r.into_inner();
        let user = uid(&r.user_id)?;
        let file_id = optional_uid(r.file_id.as_deref())?;
        let span = tracing::info_span!(
            "rag.vector_search",
            otel.kind = "server",
            user_hash = %common::observability::hash_user_id(user),
            file_id = ?file_id,
            requested_limit = r.limit
        );
        let hits = self
            .perform_vector_search(user, &r.query, r.limit, file_id)
            .instrument(span)
            .await?;
        tracing::info!(
            event = "audit",
            action = "vector_search",
            %user,
            file_id = ?file_id,
            result_count = hits.len(),
            "vector search completed"
        );
        sqlx::query("INSERT INTO file_audit_log(id,user_id,action,metadata) VALUES($1,$2,'vector_search',$3)")
            .bind(Uuid::now_v7())
            .bind(user)
            .bind(serde_json::json!({
                "file_id": file_id,
                "result_count": hits.len(),
                "query_length": r.query.chars().count(),
                "limit": r.limit
            }))
            .execute(&self.pool)
            .await
            .map_err(|e| Status::unavailable(e.to_string()))?;
        metrics::counter!("file_audit_events_total", "action" => "vector_search", "user_hash" => common::observability::hash_user_id(user)).increment(1);
        Ok(Response::new(SearchVectorsResponse { hits }))
    }

    async fn get_vector_usage(
        &self,
        r: Request<GetVectorUsageRequest>,
    ) -> Result<Response<VectorUsageResponse>, Status> {
        require_vector_caller(&r)?;
        let user = uid(&r.into_inner().user_id)?;
        let limits = self.plan_limits(user).await?;
        let collection = self
            .collection_for(user, &limits.qdrant_mode)
            .map_err(Status::failed_precondition)?;
        let row = sqlx::query_as::<_, (i64, i64, i64, chrono::DateTime<chrono::Utc>)>(
            "SELECT vector_count,vector_bytes,document_count,updated_at FROM vector_usage WHERE user_id=$1",
        )
        .bind(user)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Status::unavailable(e.to_string()))?;
        let (vector_count, vector_bytes, document_count, updated_at) =
            row.unwrap_or((0, 0, 0, chrono::Utc::now()));
        let (input_tokens, cost_micros, tokens_avoided, completed_jobs, failed_jobs) =
            sqlx::query_as::<_, (i64, i64, i64, i64, i64)>(
                "SELECT COALESCE(SUM(input_tokens),0)::BIGINT, COALESCE(SUM(cost_micros),0)::BIGINT, COALESCE(SUM(tokens_avoided),0)::BIGINT, COUNT(*) FILTER (WHERE status='completed')::BIGINT, COUNT(*) FILTER (WHERE status IN ('failed','dead_letter'))::BIGINT FROM ingestion_jobs WHERE user_id=$1",
            )
            .bind(user)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| Status::unavailable(error.to_string()))?;
        Ok(Response::new(VectorUsageResponse {
            user_id: user.to_string(),
            vector_count,
            vector_bytes,
            document_count,
            collection_mode: limits.qdrant_mode,
            collection_name: collection,
            updated_at: updated_at.to_rfc3339(),
            input_tokens,
            cost_micros,
            tokens_avoided,
            completed_jobs,
            failed_jobs,
        }))
    }

    async fn get_storage_usage(
        &self,
        r: Request<GetStorageUsageRequest>,
    ) -> Result<Response<StorageUsageResponse>, Status> {
        let user = uid(&r.into_inner().user_id)?;
        let limits = self.plan_limits(user).await?;
        let row = sqlx::query_as::<_, (i64, i64, i64, chrono::DateTime<chrono::Utc>)>(
            "SELECT used_bytes,quota_bytes,file_count,updated_at FROM file_quotas WHERE user_id=$1",
        )
        .bind(user)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| Status::unavailable(error.to_string()))?
        .unwrap_or((0, limits.storage_bytes_quota.max(0), 0, chrono::Utc::now()));
        let quota_bytes = if row.1 == 0 {
            limits.storage_bytes_quota.max(0)
        } else {
            row.1
        };
        let remaining_bytes = if quota_bytes == 0 {
            0
        } else {
            quota_bytes.saturating_sub(row.0)
        };
        Ok(Response::new(StorageUsageResponse {
            user_id: user.to_string(),
            used_bytes: row.0,
            quota_bytes,
            remaining_bytes,
            file_count: row.2,
            max_files: limits.max_files.max(0),
            updated_at: row.3.to_rfc3339(),
        }))
    }

    async fn delete_file_vectors(
        &self,
        r: Request<DeleteFileVectorsRequest>,
    ) -> Result<Response<DeleteFileVectorsResponse>, Status> {
        require_vector_caller(&r)?;
        let r = r.into_inner();
        let user = uid(&r.user_id)?;
        self.plan_limits(user).await?;
        let file =
            Uuid::parse_str(&r.file_id).map_err(|_| Status::invalid_argument("invalid file_id"))?;
        let owns_file = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM files WHERE id=$1 AND user_id=$2)",
        )
        .bind(file)
        .bind(user)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| Status::unavailable(e.to_string()))?;
        if !owns_file {
            return Err(Status::not_found("file not found"));
        }
        let deleted = self
            .delete_all_file_vectors(user, file, &r.reason)
            .await
            .map_err(Status::unavailable)?;
        Ok(Response::new(DeleteFileVectorsResponse {
            success: true,
            deleted_vectors: deleted,
        }))
    }

    async fn reindex_file_vectors(
        &self,
        r: Request<ReindexFileVectorsRequest>,
    ) -> Result<Response<ReindexFileVectorsResponse>, Status> {
        require_vector_caller(&r)?;
        let r = r.into_inner();
        let user = uid(&r.user_id)?;
        let limits = self.plan_limits(user).await?;
        if !limits.embeddings_enabled || !limits.rag_enabled || limits.qdrant_mode == "none" {
            return Err(Status::permission_denied(
                "vector indexing is disabled for the active plan",
            ));
        }
        let file =
            Uuid::parse_str(&r.file_id).map_err(|_| Status::invalid_argument("invalid file_id"))?;
        let row = sqlx::query_as::<_, (String, String, i64)>(
            "SELECT object_key,mime_type,size_bytes FROM files WHERE id=$1 AND user_id=$2 AND status='ready'",
        )
        .bind(file)
        .bind(user)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Status::unavailable(e.to_string()))?
        .ok_or_else(|| Status::not_found("ready file not found"))?;
        if limits.max_file_size_bytes > 0 && row.2 > limits.max_file_size_bytes {
            return Err(Status::resource_exhausted(
                "file exceeds the active plan limit",
            ));
        }
        let reservation_id = self
            .quota
            .reserve(user, file, row.2, Self::ingestion_limits(&limits))
            .await
            .map_err(Status::unavailable)?;
        let Some(reservation_id) = reservation_id else {
            metrics::counter!("file_ingestion_quota_rejections_total", "reason" => "plan_limit")
                .increment(1);
            return Err(Status::resource_exhausted("ingestion quota exceeded"));
        };
        let mut tx = match self.pool.begin().await {
            Ok(tx) => tx,
            Err(error) => {
                let _ = self
                    .quota
                    .release_reservation(user, reservation_id, &error.to_string())
                    .await;
                return Err(Status::unavailable(error.to_string()));
            }
        };
        if let Err(error) = sqlx::query("UPDATE ingestion_jobs SET status='pending',last_error=NULL,updated_at=now() WHERE file_id=$1 AND user_id=$2 AND strategy_version=$3 AND embedding_model=$4")
            .bind(file)
            .bind(user)
            .bind(&self.ingestion_strategy_version)
            .bind(&self.embedding_model)
            .execute(&mut *tx)
            .await
        {
            let _ = tx.rollback().await;
            let _ = self.quota.release_reservation(user, reservation_id, &error.to_string()).await;
            return Err(Status::unavailable(error.to_string()));
        }
        let event = EventEnvelope::new(
            EVENT_FILE_INGEST_REQUESTED,
            FileIngestRequested {
                file_id: file,
                user_id: user,
                object_key: row.0,
                mime_type: row.1,
                size_bytes: row.2,
                strategy_version: self.ingestion_strategy_version.clone(),
                embedding_model: self.embedding_model.clone(),
                quota_reservation_id: Some(reservation_id),
            },
        );
        if let Err(error) =
            enqueue_in_transaction(&mut tx, "event_outbox", EVENT_FILE_INGEST_REQUESTED, &event)
                .await
        {
            let _ = tx.rollback().await;
            let _ = self
                .quota
                .release_reservation(user, reservation_id, &error.to_string())
                .await;
            return Err(Status::unavailable(format!(
                "failed to enqueue reindex: {error}"
            )));
        }
        if let Err(error) = sqlx::query("INSERT INTO file_audit_log(id,user_id,file_id,action,metadata) VALUES($1,$2,$3,'vectors_reindex_requested',$4)")
            .bind(Uuid::now_v7())
            .bind(user)
            .bind(file)
            .bind(serde_json::json!({"embedding_model":self.embedding_model,"embedding_version":self.embedding_version}))
            .execute(&mut *tx)
            .await
        {
            let _ = tx.rollback().await;
            let _ = self.quota.release_reservation(user, reservation_id, &error.to_string()).await;
            return Err(Status::unavailable(error.to_string()));
        }
        if let Err(error) = tx.commit().await {
            let _ = self
                .quota
                .release_reservation(user, reservation_id, &error.to_string())
                .await;
            return Err(Status::unavailable(error.to_string()));
        }
        Ok(Response::new(ReindexFileVectorsResponse { accepted: true }))
    }
}

async fn file_metrics_handler(State(handle): State<PrometheusHandle>) -> String {
    handle.render()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = AppConfig::from_env_async().await?;
    let config_reload = common::ConfigReloadHandle::start(config.clone());
    common::observability::init("file-service", &config)?;
    let tls = ServiceTlsConfig::from_env("file-service")?;
    let url = std::env::var("FILE_SERVICE_DATABASE_URL")?;
    let pool = common::startup::connect_postgres(
        "file-service",
        &url,
        config.database_max_connections.unwrap_or(20),
        config.database_acquire_timeout_ms.unwrap_or(5000),
        config.database_idle_timeout_ms.unwrap_or(300000),
    )
    .await?;
    config_reload.register_postgres_pool("file-service", "FILE_SERVICE_DATABASE_URL", pool.clone());
    let redis = common::startup::connect_redis(
        "file-service",
        &config.redis_url,
        config.redis_pool_size.unwrap_or(16),
        config.redis_timeout_ms.unwrap_or(2000),
    )
    .await
    .context("failed to connect Redis for ingestion quotas")?;
    config_reload.register_redis_pool("file-service", "REDIS_URL", redis.clone());
    let quota = quota::QuotaManager::new(pool.clone(), redis);
    let secret = config
        .service_jwt_secret
        .clone()
        .context("SERVICE_JWT_SECRET required")?;
    let mut issuers = std::env::var("FILE_SERVICE_ALLOWED_ISSUERS")
        .unwrap_or_else(|_| "customer-bff,admin-bff".into())
        .split(',')
        .map(|s| s.trim().to_string())
        .collect::<HashSet<_>>();
    let mut subjects = std::env::var("FILE_SERVICE_ALLOWED_SUBJECTS")
        .unwrap_or_else(|_| "customer-bff,admin-bff".into())
        .split(',')
        .map(|s| s.trim().to_string())
        .collect::<HashSet<_>>();
    issuers.insert("s3-gateway".into());
    subjects.insert("s3-gateway".into());
    let auth =
        ServiceJwtAuth::with_secrets(&[secret], issuers, subjects).map_err(anyhow::Error::msg)?;
    let plan_client = PlanClient {
        endpoint: std::env::var("FILE_SERVICE_SUBSCRIPTION_SERVICE_GRPC_ENDPOINT")
            .or_else(|_| std::env::var("SUBSCRIPTION_SERVICE_GRPC_ENDPOINT"))
            .unwrap_or_else(|_| "https://127.0.0.1:50065".into()),
        secret: config
            .service_jwt_secret
            .clone()
            .context("SERVICE_JWT_SECRET required")?,
        issuer: std::env::var("FILE_SERVICE_SERVICE_JWT_ISSUER")
            .unwrap_or_else(|_| "file-service".into()),
        subject: std::env::var("FILE_SERVICE_SERVICE_JWT_SUBJECT")
            .unwrap_or_else(|_| "file-service".into()),
        tls: tls.clone(),
    };
    let r2_endpoint = std::env::var("FILE_SERVICE_R2_ENDPOINT_URL")
        .or_else(|_| std::env::var("INVOICE_R2_ENDPOINT_URL"))
        .context("FILE_SERVICE_R2_ENDPOINT_URL or INVOICE_R2_ENDPOINT_URL required")?;
    let r2_access_key = std::env::var("FILE_SERVICE_R2_ACCESS_KEY_ID")
        .or_else(|_| std::env::var("INVOICE_R2_ACCESS_KEY_ID"))
        .context("R2 access key required")?;
    let r2_secret_key = std::env::var("FILE_SERVICE_R2_SECRET_ACCESS_KEY")
        .or_else(|_| std::env::var("INVOICE_R2_SECRET_ACCESS_KEY"))
        .context("R2 secret key required")?;
    let bucket = std::env::var("FILE_SERVICE_R2_BUCKET")
        .or_else(|_| std::env::var("INVOICE_R2_BUCKET"))
        .context("FILE_SERVICE_R2_BUCKET or INVOICE_R2_BUCKET required")?;
    let credentials = Credentials::new(r2_access_key, r2_secret_key, None, None, "file-service-r2");
    let s3_config = aws_sdk_s3::Config::builder()
        .endpoint_url(r2_endpoint.trim_end_matches('/'))
        .credentials_provider(credentials)
        .region(aws_sdk_s3::config::Region::new("auto"))
        .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
        .build();
    let s3 = aws_sdk_s3::Client::from_conf(s3_config);
    s3.head_bucket()
        .bucket(&bucket)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("R2 bucket health check failed: {e}"))?;
    let embedding = Arc::new(embedding::EmbeddingService::from_env().await?);
    let embedding_cost_micros_per_1k = std::env::var("EMBEDDING_COST_MICROS_PER_1K")
        .unwrap_or_else(|_| "10".into())
        .parse::<i64>()
        .context("EMBEDDING_COST_MICROS_PER_1K must be an integer")?;
    if embedding_cost_micros_per_1k <= 0 {
        anyhow::bail!("EMBEDDING_COST_MICROS_PER_1K must be positive");
    }
    let app = App {
        pool,
        plan_client,
        s3,
        bucket,
        max_file_bytes: std::env::var("FILE_SERVICE_MAX_FILE_SIZE_BYTES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(268435456),
        upload_url_ttl_seconds: std::env::var("FILE_SERVICE_UPLOAD_URL_TTL_SECONDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(900)
            .clamp(60, 900),
        embedding_model: embedding.model().to_owned(),
        ingestion_strategy_version: std::env::var("INGESTION_STRATEGY_VERSION")
            .unwrap_or_else(|_| embedding::QWEN_STRATEGY_VERSION.into()),
        embedding,
        embedding_cost_micros_per_1k,
        embedding_http: reqwest::Client::builder()
            .pool_max_idle_per_host(32)
            .build()?,
        qdrant_url: std::env::var("QDRANT_URL").unwrap_or_else(|_| "http://127.0.0.1:6333".into()),
        qdrant_api_key: std::env::var("QDRANT_API_KEY")
            .or_else(|_| std::env::var("QDRANT__SERVICE__API_KEY"))
            .ok()
            .filter(|value| !value.trim().is_empty()),
        qdrant_collection: std::env::var("QDRANT_EMBEDDING_COLLECTION")
            .unwrap_or_else(|_| "nexus_embeddings_v2".into()),
        embedding_version: std::env::var("EMBEDDING_VERSION")
            .unwrap_or_else(|_| "qwen3-embedding-8b-v1".into()),
        embedding_dedup_lease_seconds: std::env::var("EMBEDDING_DEDUP_LEASE_SECONDS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(180)
            .clamp(30, 900),
        embedding_max_file_filter_ids: std::env::var("EMBEDDING_MAX_FILE_FILTER_IDS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(20_000)
            .clamp(100, 100_000),
        dual_write_v1: std::env::var("FILE_SERVICE_DUAL_WRITE_V1")
            .ok()
            .is_some_and(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes"
                )
            }),
        vector_read_mode: std::env::var("FILE_SERVICE_VECTOR_READ_MODE")
            .unwrap_or_else(|_| "v2".into())
            .trim()
            .to_ascii_lowercase(),
        legacy_qdrant_collection: std::env::var("QDRANT_EMBEDDING_COLLECTION_V1")
            .unwrap_or_else(|_| "nexus_embeddings".into()),
        qdrant_ready_collections: Arc::new(tokio::sync::RwLock::new(HashSet::new())),
        quota,
    };
    let ingestion_ack_pending = std::env::var("FILE_SERVICE_INGESTION_MAX_ACK_PENDING")
        .unwrap_or_else(|_| "32".into())
        .parse::<usize>()
        .context("FILE_SERVICE_INGESTION_MAX_ACK_PENDING must be a positive integer")?
        .clamp(1, 1000);
    let ingestion_batch_size = std::env::var("FILE_SERVICE_INGESTION_BATCH_SIZE")
        .unwrap_or_else(|_| "1".into())
        .parse::<usize>()
        .context("FILE_SERVICE_INGESTION_BATCH_SIZE must be a positive integer")?
        .clamp(1, 32);
    let vector_delete_ack_pending = std::env::var("FILE_SERVICE_VECTOR_DELETE_MAX_ACK_PENDING")
        .unwrap_or_else(|_| "100".into())
        .parse::<usize>()
        .context("FILE_SERVICE_VECTOR_DELETE_MAX_ACK_PENDING must be a positive integer")?
        .clamp(1, 1000);
    let vector_delete_batch_size = std::env::var("FILE_SERVICE_VECTOR_DELETE_BATCH_SIZE")
        .unwrap_or_else(|_| "32".into())
        .parse::<usize>()
        .context("FILE_SERVICE_VECTOR_DELETE_BATCH_SIZE must be a positive integer")?
        .clamp(1, 100);
    metrics::gauge!("file_ingestion_max_ack_pending").set(ingestion_ack_pending as f64);
    metrics::gauge!("file_ingestion_batch_size").set(ingestion_batch_size as f64);
    metrics::gauge!("file_vector_delete_max_ack_pending").set(vector_delete_ack_pending as f64);
    metrics::gauge!("file_vector_delete_batch_size").set(vector_delete_batch_size as f64);
    if !matches!(app.vector_read_mode.as_str(), "v1" | "v2" | "dual") {
        anyhow::bail!("FILE_SERVICE_VECTOR_READ_MODE must be v1, v2 or dual");
    }
    if app.legacy_qdrant_collection.is_empty()
        || !app
            .legacy_qdrant_collection
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        anyhow::bail!("QDRANT_EMBEDDING_COLLECTION_V1 contains an invalid collection name");
    }
    let metrics_handle = PrometheusBuilder::new()
        .install_recorder()
        .context("failed to install file-service metrics recorder")?;
    let metrics_bind =
        std::env::var("FILE_SERVICE_METRICS_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0".into());
    let metrics_port: u16 = std::env::var("FILE_SERVICE_METRICS_PORT")
        .unwrap_or_else(|_| "9101".into())
        .parse()
        .context("FILE_SERVICE_METRICS_PORT must be a valid port")?;
    let metrics_addr: SocketAddr = format!("{metrics_bind}:{metrics_port}")
        .parse()
        .context("invalid file-service metrics bind address")?;
    if let Err(error) = app.refresh_observability_metrics().await {
        tracing::warn!(%error, "initial file-service observability refresh failed");
    }
    tokio::spawn(async move {
        let result = axum::serve(
            TcpListener::bind(metrics_addr).await?,
            Router::new()
                .route("/metrics", get(file_metrics_handler))
                .with_state(metrics_handle),
        )
        .await;
        result.map_err(anyhow::Error::from)
    });
    tracing::info!(%metrics_addr, "file-service metrics listening");
    let relay_nats = common::connect_nats(&config.nats_url)
        .await
        .context("failed to connect NATS for file outbox relay")?;
    config_reload.register_nats_client("file-service-outbox", "NATS_URL", relay_nats.clone());
    let relay_context = async_nats::jetstream::new(relay_nats);
    ensure_streams(&relay_context)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let relay = events::outbox::OutboxRelay::new(
        app.pool.clone(),
        relay_context,
        events::outbox::OutboxConfig::for_service("file-service", "event_outbox"),
    )
    .context("failed to configure file outbox relay")?;
    tokio::spawn(relay.run());
    let worker_app = app.clone();
    let nats_url = config.nats_url.clone();
    let worker_reload = config_reload.clone();
    tokio::spawn(async move {
        if let Err(error) = run_ingestion_worker(
            worker_app,
            &nats_url,
            worker_reload,
            ingestion_ack_pending,
            ingestion_batch_size,
        )
        .await
        {
            tracing::error!(%error, "file ingestion worker stopped");
        }
    });
    let delete_worker_app = app.clone();
    let delete_nats_url = config.nats_url.clone();
    let delete_reload = config_reload.clone();
    tokio::spawn(async move {
        if let Err(error) = run_vector_delete_worker(
            delete_worker_app,
            &delete_nats_url,
            delete_reload,
            vector_delete_ack_pending,
            vector_delete_batch_size,
        )
        .await
        {
            tracing::error!(%error, "file vector deletion worker stopped");
        }
    });
    let maintenance_app = app.clone();
    tokio::spawn(run_canonical_maintenance(maintenance_app));
    let addr: SocketAddr = format!(
        "{}:{}",
        std::env::var("FILE_SERVICE_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1".into()),
        config.grpc_port.unwrap_or(50071)
    )
    .parse()?;
    let interceptor = move |mut req: Request<()>| {
        let raw = req
            .metadata()
            .get("authorization")
            .ok_or_else(|| Status::unauthenticated("missing authorization"))?
            .to_str()
            .map_err(|_| Status::unauthenticated("invalid authorization"))?
            .strip_prefix("Bearer ")
            .ok_or_else(|| Status::unauthenticated("bearer required"))?;
        let claims = auth
            .verify(raw)
            .map_err(|_| Status::unauthenticated("invalid service token"))?;
        req.extensions_mut().insert(claims);
        Ok(req)
    };
    common::tls::validate_server_tls(&tls).await?;
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, "file-service gRPC listening");
    if tls.is_enabled() {
        let incoming = common::tls::dynamic_tls_incoming(listener, tls.clone());
        tonic::transport::Server::builder()
            .add_service(FileServiceServer::with_interceptor(app, interceptor))
            .serve_with_incoming_shutdown(incoming, common::shutdown::shutdown_signal())
            .await?;
    } else {
        let incoming = common::tls::tcp_incoming(listener);
        tonic::transport::Server::builder()
            .add_service(FileServiceServer::with_interceptor(app, interceptor))
            .serve_with_incoming_shutdown(incoming, common::shutdown::shutdown_signal())
            .await?;
    }
    Ok(())
}

async fn run_ingestion_worker(
    app: App,
    nats_url: &str,
    reload: common::ConfigReloadHandle,
    max_ack_pending: usize,
    batch_size: usize,
) -> anyhow::Result<()> {
    let client = common::connect_nats(nats_url)
        .await
        .context("failed to connect NATS for ingestion")?;
    reload.register_nats_client("file-service-ingestion", "NATS_URL", client.clone());
    let context = async_nats::jetstream::new(client);
    ensure_streams(&context)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let stream = context.get_stream(EVENTS_STREAM).await?;
    let consumer_name = "file-ingestion-worker";
    let consumer = stream
        .create_consumer(
            pull_consumer_config(
                consumer_name,
                EVENT_FILE_INGEST_REQUESTED,
                max_ack_pending as i64,
            )
            .map_err(|e| anyhow::anyhow!(e.to_string()))?,
        )
        .await?;
    tracing::info!(
        consumer = consumer_name,
        subject = EVENT_FILE_INGEST_REQUESTED,
        "file ingestion JetStream consumer started"
    );
    loop {
        // Pull one message at a time. JetStream's max_ack_pending and the
        // per-plan quota provide backpressure without buffering documents.
        let mut batch = consumer.batch().max_messages(batch_size).messages().await?;
        while let Some(message) = batch.next().await {
            let message = message.map_err(|error| anyhow::anyhow!(error.to_string()))?;
            let attempts = delivery_attempts(&message);
            if attempts > 1 {
                metrics::counter!("file_ingestion_redeliveries_total").increment(1);
            }
            metrics::gauge!("file_ingestion_messages_in_flight").set(1.0);
            let envelope: Result<EventEnvelope<FileIngestRequested>, _> =
                serde_json::from_slice(&message.payload);
            let (event, result) = match envelope {
                Ok(event) => {
                    let span = common::observability::nats_consumer_span(
                        EVENT_FILE_INGEST_REQUESTED,
                        consumer_name,
                        message.headers.as_ref(),
                    );
                    let result = app
                        .process_ingestion(event.payload.clone())
                        .instrument(span)
                        .await;
                    (Some(event), result)
                }
                Err(e) => (None, Err(format!("invalid ingestion envelope: {e}"))),
            };
            match result {
                Ok(()) => {
                    metrics::counter!("file_ingestion_completed_total").increment(1);
                    ack(&message)
                        .await
                        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                }
                Err(error) => {
                    let dead_letter = delivery_attempts(&message) >= DEFAULT_MAX_DELIVERIES;
                    if let Some(event) = event.as_ref() {
                        let plan = match app.plan_limits(event.payload.user_id).await {
                            Ok(limits) if !limits.plan_id.trim().is_empty() => limits.plan_id,
                            Ok(limits) if !limits.plan_name.trim().is_empty() => limits.plan_name,
                            Ok(limits) => {
                                drop(limits);
                                "unknown".to_owned()
                            }
                            Err(error) => {
                                drop(error);
                                "unavailable".to_owned()
                            }
                        };
                        metrics::counter!(
                            "embedding_operations_total",
                            "plan" => plan,
                            "result" => if dead_letter { "dead_letter" } else { "failed" },
                            "user_hash" => common::observability::hash_user_id(event.payload.user_id)
                        )
                        .increment(1);
                    }
                    if error.contains("quota") || error.contains("concurrency") {
                        metrics::counter!("file_ingestion_backpressure_total", "reason" => "quota")
                            .increment(1);
                    }
                    if let Some(event) = event.as_ref() {
                        app.mark_ingestion_failure(&event.payload, &error, dead_letter)
                            .await;
                        if let Some(reservation_id) = event.payload.quota_reservation_id {
                            if error == "ingestion concurrency limit reached" {
                                // The reservation remains pending; retry without
                                // charging a second file or token budget.
                            } else if dead_letter {
                                if let Err(quota_error) = app
                                    .quota
                                    .release_reservation(
                                        event.payload.user_id,
                                        reservation_id,
                                        &error,
                                    )
                                    .await
                                {
                                    tracing::error!(%quota_error, "failed to release dead-letter ingestion quota");
                                }
                            } else if let Err(quota_error) = app
                                .quota
                                .pause(event.payload.user_id, reservation_id, &error)
                                .await
                            {
                                tracing::error!(%quota_error, "failed to return ingestion quota reservation to pending");
                            }
                        }
                    }
                    metrics::counter!(
                        "file_ingestion_failures_total",
                        "dead_letter" => dead_letter.to_string()
                    )
                    .increment(1);
                    retry_or_dead_letter(&context, &message, consumer_name, error)
                        .await
                        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                }
            }
            metrics::gauge!("file_ingestion_messages_in_flight").set(0.0);
        }
    }
}

async fn run_vector_delete_worker(
    app: App,
    nats_url: &str,
    reload: common::ConfigReloadHandle,
    max_ack_pending: usize,
    batch_size: usize,
) -> anyhow::Result<()> {
    let client = common::connect_nats(nats_url)
        .await
        .context("failed to connect NATS for vector deletion")?;
    reload.register_nats_client("file-service-vector-delete", "NATS_URL", client.clone());
    let context = async_nats::jetstream::new(client);
    ensure_streams(&context)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let stream = context.get_stream(EVENTS_STREAM).await?;
    let consumer_name = "file-vector-delete-worker";
    let consumer = stream
        .create_consumer(
            pull_consumer_config(
                consumer_name,
                EVENT_FILE_VECTORS_DELETE_REQUESTED,
                max_ack_pending as i64,
            )
            .map_err(|e| anyhow::anyhow!(e.to_string()))?,
        )
        .await?;
    tracing::info!(
        consumer = consumer_name,
        subject = EVENT_FILE_VECTORS_DELETE_REQUESTED,
        "file vector deletion JetStream consumer started"
    );
    loop {
        let mut batch = consumer.batch().max_messages(batch_size).messages().await?;
        while let Some(message) = batch.next().await {
            let message = message.map_err(|error| anyhow::anyhow!(error.to_string()))?;
            let envelope: Result<EventEnvelope<FileVectorsDeleteRequested>, _> =
                serde_json::from_slice(&message.payload);
            let result = match envelope {
                Ok(event) => app
                    .delete_all_file_vectors(
                        event.payload.user_id,
                        event.payload.file_id,
                        &event.payload.reason,
                    )
                    .await
                    .map(|_| ()),
                Err(error) => Err(format!("invalid vector deletion envelope: {error}")),
            };
            match result {
                Ok(()) => {
                    ack(&message)
                        .await
                        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                }
                Err(error) => {
                    retry_or_dead_letter(&context, &message, consumer_name, error)
                        .await
                        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                }
            }
        }
    }
}

async fn run_canonical_maintenance(app: App) {
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        match app.sync_pending_canonical_vectors(500).await {
            Ok(count) if count > 0 => {
                metrics::counter!("canonical_embedding_backfill_synced_total")
                    .increment(count as u64);
                tracing::info!(
                    vectors = count,
                    "canonical embedding vectors synchronized with Qdrant"
                );
            }
            Ok(_) => {}
            Err(error) => {
                metrics::counter!("canonical_embedding_maintenance_failures_total",
                    "operation" => "qdrant_sync")
                .increment(1);
                tracing::error!(%error, "canonical Qdrant synchronization failed");
            }
        }
        match canonical::pending_deletion_owners(&app.pool).await {
            Ok(owners) => {
                for owner in owners {
                    if let Err(error) = app.flush_pending_canonical_deletions(owner).await {
                        metrics::counter!("canonical_embedding_maintenance_failures_total",
                            "operation" => "qdrant_delete")
                        .increment(1);
                        tracing::error!(user_id=%owner, %error, "canonical Qdrant deletion retry failed");
                    }
                }
            }
            Err(error) => {
                tracing::error!(%error, "failed to load pending canonical vector deletions");
            }
        }
        if let Err(error) = app.refresh_observability_metrics().await {
            tracing::warn!(%error, "file-service observability refresh failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        canonical_ownership_filter, ownership_filter, qdrant_collection_name,
        qdrant_payload_belongs_to_user,
    };
    use uuid::Uuid;

    #[test]
    fn shared_collection_never_contains_user_controlled_input() {
        let user = Uuid::nil();
        assert_eq!(
            qdrant_collection_name("nexus_embeddings_v2", user, "shared").unwrap(),
            "nexus_embeddings_v2"
        );
    }

    #[test]
    fn dedicated_collection_is_deterministically_scoped_to_user() {
        let user = Uuid::parse_str("019f935a-4d42-7c51-a414-2b5adee3a3c9").unwrap();
        assert_eq!(
            qdrant_collection_name("nexus_embeddings_v2", user, "dedicated").unwrap(),
            "nexus_embeddings_v2_019f935a4d427c51a4142b5adee3a3c9"
        );
    }

    #[test]
    fn ownership_filter_always_contains_user_id() {
        let user = Uuid::now_v7();
        let file = Uuid::now_v7();
        let filter = ownership_filter(user, Some(file));
        let must = filter["must"].as_array().unwrap();
        assert_eq!(must[0]["key"], "user_id");
        assert_eq!(must[0]["match"]["value"], user.to_string());
        assert_eq!(must[1]["key"], "file_id");
        assert_eq!(must[1]["match"]["value"], file.to_string());
    }

    #[test]
    fn unsupported_collection_mode_fails_closed() {
        assert!(
            qdrant_collection_name("nexus_embeddings_v2", Uuid::nil(), "client-value").is_err()
        );
    }

    #[test]
    fn legacy_collection_schema_fails_closed() {
        assert!(qdrant_collection_name("nexus_embeddings", Uuid::nil(), "shared").is_err());
        assert!(qdrant_collection_name("nexus_embeddings_v3", Uuid::nil(), "shared").is_err());
    }

    #[test]
    fn canonical_filter_always_scopes_owner_and_requested_points() {
        let user = Uuid::now_v7();
        let point_a = Uuid::now_v7();
        let point_b = Uuid::now_v7();
        let filter = canonical_ownership_filter(user, &[point_a, point_b]);
        let must = filter["must"].as_array().unwrap();
        assert_eq!(must.len(), 2);
        assert_eq!(must[0]["key"], "user_id");
        assert_eq!(must[0]["match"]["value"], user.to_string());
        assert_eq!(
            must[1]["has_id"],
            serde_json::json!([point_a.to_string(), point_b.to_string()])
        );
    }

    #[test]
    fn canonical_filter_without_point_list_still_scopes_owner() {
        let user = Uuid::now_v7();
        let filter = canonical_ownership_filter(user, &[]);
        let must = filter["must"].as_array().unwrap();
        assert_eq!(must.len(), 1);
        assert_eq!(must[0]["key"], "user_id");
        assert_eq!(must[0]["match"]["value"], user.to_string());
    }

    #[test]
    fn qdrant_foreign_payload_is_rejected_defensively() {
        let owner = Uuid::now_v7();
        let foreign = Uuid::now_v7();
        let payload = serde_json::json!({"user_id": foreign.to_string()});
        assert!(!qdrant_payload_belongs_to_user(&payload, owner));
        assert!(qdrant_payload_belongs_to_user(
            &serde_json::json!({"user_id": owner.to_string()}),
            owner
        ));
    }

    #[test]
    fn qdrant_payload_without_owner_is_rejected() {
        assert!(!qdrant_payload_belongs_to_user(
            &serde_json::json!({"embedding_vector_id": Uuid::now_v7().to_string()}),
            Uuid::now_v7()
        ));
    }

    #[tokio::test]
    #[ignore = "requires a migrated FILE_SERVICE_DATABASE_URL PostgreSQL instance"]
    async fn postgres_owner_predicate_rejects_cross_user_file_access(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let database_url = std::env::var("FILE_SERVICE_DATABASE_URL")?;
        let pool = sqlx::PgPool::connect(&database_url).await?;
        let owner = Uuid::now_v7();
        let foreign = Uuid::now_v7();
        let file = Uuid::now_v7();
        sqlx::query("INSERT INTO files(id,user_id,name,mime_type,size_bytes,sha256,status,object_key,idempotency_key) VALUES($1,$2,'isolation-test.txt','text/plain',1,$3,'pending',$4,$5)")
            .bind(file)
            .bind(owner)
            .bind("0".repeat(64))
            .bind(format!("tests/{file}"))
            .bind(format!("isolation-{file}"))
            .execute(&pool)
            .await?;
        let visible: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM files WHERE id=$1 AND user_id=$2 AND status <> 'deleted'",
        )
        .bind(file)
        .bind(foreign)
        .fetch_optional(&pool)
        .await?;
        sqlx::query("DELETE FROM files WHERE id=$1 AND user_id=$2")
            .bind(file)
            .bind(owner)
            .execute(&pool)
            .await?;
        if visible.is_some() {
            return Err("cross-user file access was visible".into());
        }
        Ok(())
    }
}
