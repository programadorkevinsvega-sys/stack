use sqlx::{FromRow, PgPool, Postgres, Transaction};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct UniqueChunk {
    pub content_hash: String,
    pub content: String,
    pub token_count: i32,
}

#[derive(Clone, Debug, FromRow)]
pub struct CanonicalVector {
    pub id: Uuid,
    pub user_id: Uuid,
    pub content_hash: String,
    pub content: String,
    pub embedding_model: String,
    pub embedding_version: String,
    pub strategy_family: String,
    pub embedding: Option<serde_json::Value>,
    pub token_count: i32,
    pub vector_dimension: Option<i32>,
    pub vector_bytes: i64,
    pub collection_name: String,
    pub qdrant_point_id: Uuid,
    pub qdrant_synced: bool,
    pub status: String,
    pub lease_owner: Option<Uuid>,
}

#[derive(Debug)]
pub struct Reservation {
    pub worker_id: Uuid,
    pub owned: Vec<CanonicalVector>,
}

#[derive(Clone, Debug)]
pub struct ChunkReference {
    pub chunk_id: Uuid,
    pub chunk_index: i32,
    pub vector_id: Uuid,
}

#[derive(Clone, Copy, Debug)]
pub struct IngestionAccounting {
    pub embedding_count: i32,
    pub input_tokens: i64,
    pub cost_micros: i64,
    pub cache_hits: i64,
    pub cache_misses: i64,
    pub tokens_avoided: i64,
    pub cost_avoided_micros: i64,
}

pub struct FileReferenceCommit<'a> {
    pub user_id: Uuid,
    pub file_id: Uuid,
    pub strategy_version: &'a str,
    pub embedding_model: &'a str,
    pub embedding_version: &'a str,
    pub job_id: Uuid,
    pub accounting: IngestionAccounting,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReferenceCommitStats {
    pub retained: i64,
    pub added: i64,
    pub removed: i64,
}

#[derive(Clone, Debug, FromRow)]
pub struct PendingDeletion {
    pub embedding_vector_id: Uuid,
    pub user_id: Uuid,
    pub collection_name: String,
    pub qdrant_point_id: Uuid,
}

#[derive(Clone, Debug, FromRow)]
pub struct SearchReference {
    pub embedding_vector_id: Uuid,
    pub file_id: Uuid,
    pub document_id: Uuid,
    pub chunk_id: Uuid,
    pub chunk_index: i32,
    pub content: String,
    pub content_hash: String,
    pub embedding_model: String,
    pub embedding_version: String,
}

const VECTOR_COLUMNS: &str = "
    id,user_id,content_hash,content,embedding_model,embedding_version,strategy_family,embedding,
    token_count,vector_dimension,vector_bytes,collection_name,qdrant_point_id,
    qdrant_synced,status,lease_owner";

pub struct ReserveVectorsInput<'a> {
    pub pool: &'a PgPool,
    pub user_id: Uuid,
    pub embedding_model: &'a str,
    pub embedding_version: &'a str,
    pub strategy_family: &'a str,
    pub collection_name: &'a str,
    pub chunks: &'a [UniqueChunk],
    pub lease_seconds: i32,
}

pub async fn reserve_vectors<'input>(
    input: ReserveVectorsInput<'input>,
) -> Result<Reservation, String> {
    let ReserveVectorsInput {
        pool,
        user_id,
        embedding_model,
        embedding_version,
        strategy_family,
        collection_name,
        chunks,
        lease_seconds,
    } = input;
    let worker_id = Uuid::now_v7();
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;

    for chunk in chunks {
        let vector_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO embedding_vectors(
                id,user_id,content_hash,content,embedding_model,embedding_version,
                token_count,collection_name,strategy_family,qdrant_point_id,status,lease_owner,
                lease_until
             )
             VALUES(
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$1,'embedding',$10,
                now() + make_interval(secs => $11)
             )
             ON CONFLICT(user_id,content_hash,embedding_model,embedding_version)
             DO NOTHING",
        )
        .bind(vector_id)
        .bind(user_id)
        .bind(&chunk.content_hash)
        .bind(&chunk.content)
        .bind(embedding_model)
        .bind(embedding_version)
        .bind(chunk.token_count)
        .bind(collection_name)
        .bind(strategy_family)
        .bind(worker_id)
        .bind(lease_seconds)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;

        let expired: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)::BIGINT
             FROM embedding_vectors
             WHERE user_id=$1 AND content_hash=$2 AND embedding_model=$3
               AND embedding_version=$4
               AND status IN ('reserved','embedding')
               AND lease_until < now()",
        )
        .bind(user_id)
        .bind(&chunk.content_hash)
        .bind(embedding_model)
        .bind(embedding_version)
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
        if expired > 0 {
            metrics::counter!("canonical_embedding_lease_expired_total").increment(expired as u64);
        }

        sqlx::query(
            "UPDATE embedding_vectors
             SET status='embedding',
                 lease_owner=$1,
                 lease_until=now() + make_interval(secs => $2),
                 collection_name=$3,
                 last_error=NULL,
                 updated_at=now()
             WHERE user_id=$4
               AND content_hash=$5
               AND embedding_model=$6
               AND embedding_version=$7
               AND (
                    status='failed'
                    OR (
                        status IN ('reserved','embedding')
                        AND (lease_until IS NULL OR lease_until < now())
                    )
               )",
        )
        .bind(worker_id)
        .bind(lease_seconds)
        .bind(collection_name)
        .bind(user_id)
        .bind(&chunk.content_hash)
        .bind(embedding_model)
        .bind(embedding_version)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    }

    let hashes = chunks
        .iter()
        .map(|chunk| chunk.content_hash.clone())
        .collect::<Vec<_>>();
    let rows = sqlx::query_as::<_, CanonicalVector>(&format!(
        "SELECT {VECTOR_COLUMNS}
         FROM embedding_vectors
         WHERE user_id=$1
           AND embedding_model=$2
           AND embedding_version=$3
           AND content_hash::TEXT = ANY($4)
         FOR UPDATE"
    ))
    .bind(user_id)
    .bind(embedding_model)
    .bind(embedding_version)
    .bind(&hashes)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;

    if rows.len() != chunks.len() {
        return Err("canonical embedding reservation returned an incomplete set".into());
    }

    let expected_content = chunks
        .iter()
        .map(|chunk| (chunk.content_hash.as_str(), chunk.content.as_str()))
        .collect::<HashMap<_, _>>();
    let mut owned = Vec::new();
    for row in rows {
        if expected_content.get(row.content_hash.as_str()).copied() != Some(row.content.as_str()) {
            return Err("content hash collision detected in canonical embedding cache".into());
        }
        match row.status.as_str() {
            "ready" => {}
            "embedding" if row.lease_owner == Some(worker_id) => owned.push(row),
            "embedding" | "reserved" => {
                return Err("canonical embedding is being generated by another worker".into());
            }
            "deleting" => {
                return Err("canonical embedding is being deleted; retry ingestion".into());
            }
            state => {
                return Err(format!(
                    "canonical embedding is unavailable in state {state}"
                ));
            }
        }
    }
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(Reservation { worker_id, owned })
}

pub async fn complete_owned_vectors(
    pool: &PgPool,
    worker_id: Uuid,
    vectors: &[(Uuid, Vec<f32>)],
) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    for (id, embedding) in vectors {
        if embedding.is_empty() || embedding.iter().any(|value| !value.is_finite()) {
            return Err("embedding provider returned an invalid canonical vector".into());
        }
        let dimension =
            i32::try_from(embedding.len()).map_err(|_| "embedding dimension overflow")?;
        let bytes = i64::try_from(embedding.len() * std::mem::size_of::<f32>())
            .map_err(|_| "embedding byte size overflow")?;
        let changed = sqlx::query(
            "UPDATE embedding_vectors
             SET embedding=$1,
                 vector_dimension=$2,
                 vector_bytes=$3,
                 status='ready',
                 qdrant_synced=false,
                 lease_owner=NULL,
                 lease_until=NULL,
                 last_error=NULL,
                 updated_at=now()
             WHERE id=$4 AND status='embedding' AND lease_owner=$5",
        )
        .bind(serde_json::to_value(embedding).map_err(|error| error.to_string())?)
        .bind(dimension)
        .bind(bytes)
        .bind(id)
        .bind(worker_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
        if changed.rows_affected() != 1 {
            return Err("canonical embedding lease was lost before completion".into());
        }
    }
    tx.commit().await.map_err(|error| error.to_string())
}

pub async fn mark_owned_failed(
    pool: &PgPool,
    worker_id: Uuid,
    error: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE embedding_vectors
         SET status='failed',
             last_error=$1,
             lease_owner=NULL,
             lease_until=NULL,
             updated_at=now()
         WHERE status='embedding' AND lease_owner=$2",
    )
    .bind(error)
    .bind(worker_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn load_ready_vectors(
    pool: &PgPool,
    user_id: Uuid,
    embedding_model: &str,
    embedding_version: &str,
    hashes: &[String],
) -> Result<Vec<CanonicalVector>, String> {
    let rows = sqlx::query_as::<_, CanonicalVector>(&format!(
        "SELECT {VECTOR_COLUMNS}
         FROM embedding_vectors
         WHERE user_id=$1
           AND embedding_model=$2
           AND embedding_version=$3
           AND content_hash::TEXT = ANY($4)
           AND status='ready'"
    ))
    .bind(user_id)
    .bind(embedding_model)
    .bind(embedding_version)
    .bind(hashes)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    if rows.len() != hashes.len() {
        return Err("not all canonical embeddings became ready".into());
    }
    Ok(rows)
}

pub async fn mark_qdrant_synced(
    pool: &PgPool,
    user_id: Uuid,
    collection_name: &str,
    vector_ids: &[Uuid],
) -> Result<(), String> {
    let changed = sqlx::query(
        "UPDATE embedding_vectors
         SET qdrant_synced=true,collection_name=$1,updated_at=now()
         WHERE user_id=$2 AND id = ANY($3) AND status='ready'",
    )
    .bind(collection_name)
    .bind(user_id)
    .bind(vector_ids)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    if changed.rows_affected() != vector_ids.len() as u64 {
        return Err("failed to mark every canonical vector as Qdrant-synchronized".into());
    }
    Ok(())
}

pub async fn commit_file_references(
    pool: &PgPool,
    references: &[ChunkReference],
    commit: FileReferenceCommit<'_>,
) -> Result<ReferenceCommitStats, String> {
    let FileReferenceCommit {
        user_id,
        file_id,
        strategy_version,
        embedding_model,
        embedding_version,
        job_id,
        accounting,
    } = commit;
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    lock_owned_file(&mut tx, user_id, file_id).await?;
    let previous = sqlx::query_as::<_, (Uuid, Uuid, i32, String, String, String)>(
        "SELECT id, embedding_vector_id, chunk_index, strategy_version,
                embedding_model, embedding_version
         FROM embedding_chunk_refs
         WHERE user_id=$1 AND file_id=$2",
    )
    .bind(user_id)
    .bind(file_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;

    let mut incoming_indices = HashSet::with_capacity(references.len());
    for reference in references {
        if !incoming_indices.insert(reference.chunk_index) {
            return Err(format!(
                "duplicate chunk index {} in ingestion result",
                reference.chunk_index
            ));
        }
    }
    let mut current = HashMap::<i32, (Uuid, Uuid)>::new();
    let mut delete_ids = Vec::new();
    let mut affected = HashSet::new();
    for (reference_id, vector_id, chunk_index, strategy, model, version) in previous {
        affected.insert(vector_id);
        if strategy == strategy_version
            && model == embedding_model
            && version == embedding_version
            && incoming_indices.contains(&chunk_index)
        {
            current.insert(chunk_index, (reference_id, vector_id));
        } else {
            delete_ids.push(reference_id);
        }
    }
    let mut stats = ReferenceCommitStats::default();
    for reference in references {
        if let Some((reference_id, old_vector_id)) = current.remove(&reference.chunk_index) {
            affected.insert(reference.vector_id);
            if old_vector_id == reference.vector_id {
                stats.retained += 1;
            } else {
                let changed = sqlx::query(
                    "UPDATE embedding_chunk_refs
                     SET embedding_vector_id=$1, chunk_id=$2
                     WHERE id=$3 AND user_id=$4 AND file_id=$5
                       AND EXISTS(
                           SELECT 1 FROM embedding_vectors
                           WHERE id=$1 AND user_id=$4
                             AND status='ready' AND qdrant_synced=true
                       )",
                )
                .bind(reference.vector_id)
                .bind(reference.chunk_id)
                .bind(reference_id)
                .bind(user_id)
                .bind(file_id)
                .execute(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;
                if changed.rows_affected() != 1 {
                    return Err("failed to update changed canonical chunk reference".into());
                }
                stats.added += 1;
            }
            continue;
        }

        let inserted = sqlx::query(
            "INSERT INTO embedding_chunk_refs(
                id,user_id,file_id,document_id,chunk_id,chunk_index,
                strategy_version,embedding_model,embedding_version,
                embedding_vector_id
             )
             SELECT $1,$2,$3,$3,$4,$5,$6,$7,$8,$9
             WHERE EXISTS(
                SELECT 1
                FROM embedding_vectors
                WHERE id=$9 AND user_id=$2 AND status='ready'
                  AND qdrant_synced=true
             )",
        )
        .bind(Uuid::now_v7())
        .bind(user_id)
        .bind(file_id)
        .bind(reference.chunk_id)
        .bind(reference.chunk_index)
        .bind(strategy_version)
        .bind(embedding_model)
        .bind(embedding_version)
        .bind(reference.vector_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
        if inserted.rows_affected() != 1 {
            return Err("failed to persist canonical chunk reference".into());
        }
        stats.added += 1;
        affected.insert(reference.vector_id);
    }

    // Rows from older strategies/models and current-scope rows that were not
    // consumed above are no longer part of the source document.
    delete_ids.extend(current.into_values().map(|(reference_id, _)| reference_id));
    delete_ids.sort_unstable();
    delete_ids.dedup();
    if !delete_ids.is_empty() {
        let deleted = sqlx::query(
            "DELETE FROM embedding_chunk_refs
             WHERE user_id=$1 AND file_id=$2 AND id = ANY($3)",
        )
        .bind(user_id)
        .bind(file_id)
        .bind(&delete_ids)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
        if deleted.rows_affected() != delete_ids.len() as u64 {
            return Err("canonical reference set changed concurrently".into());
        }
        stats.removed += delete_ids.len() as i64;
    }

    affected.extend(references.iter().map(|reference| reference.vector_id));
    let affected = affected.into_iter().collect::<Vec<_>>();
    refresh_reference_counts(&mut tx, &affected).await?;
    queue_orphans(&mut tx, user_id, &affected).await?;
    refresh_vector_usage(&mut tx, user_id).await?;

    sqlx::query(
        "UPDATE ingestion_jobs
         SET status='completed',
             embedding_count=$1,
             input_tokens=$2,
             cost_micros=$3,
             cache_hits=$4,
             cache_misses=$5,
             tokens_avoided=$6,
             cost_avoided_micros=$7,
             chunks_retained=$8,
             chunks_added=$9,
             chunks_removed=$10,
             completed_at=now(),
             updated_at=now()
         WHERE id=$11",
    )
    .bind(accounting.embedding_count)
    .bind(accounting.input_tokens)
    .bind(accounting.cost_micros)
    .bind(accounting.cache_hits)
    .bind(accounting.cache_misses)
    .bind(accounting.tokens_avoided)
    .bind(accounting.cost_avoided_micros)
    .bind(stats.retained)
    .bind(stats.added)
    .bind(stats.removed)
    .bind(job_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;

    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(stats)
}

pub async fn remove_file_references(
    pool: &PgPool,
    user_id: Uuid,
    file_id: Uuid,
    reason: &str,
) -> Result<i64, String> {
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    lock_owned_file(&mut tx, user_id, file_id).await?;
    let affected = sqlx::query_scalar::<_, Uuid>(
        "SELECT embedding_vector_id
         FROM embedding_chunk_refs
         WHERE user_id=$1 AND file_id=$2
         FOR UPDATE",
    )
    .bind(user_id)
    .bind(file_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    let deleted_refs =
        sqlx::query("DELETE FROM embedding_chunk_refs WHERE user_id=$1 AND file_id=$2")
            .bind(user_id)
            .bind(file_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?
            .rows_affected() as i64;

    let legacy_deleted = sqlx::query("DELETE FROM embedding_chunks WHERE user_id=$1 AND file_id=$2")
        .bind(user_id)
        .bind(file_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?
        .rows_affected() as i64;

    let affected = affected.into_iter().collect::<HashSet<_>>();
    let affected = affected.into_iter().collect::<Vec<_>>();
    refresh_reference_counts(&mut tx, &affected).await?;
    queue_orphans(&mut tx, user_id, &affected).await?;
    refresh_vector_usage(&mut tx, user_id).await?;
    sqlx::query(
        "INSERT INTO file_audit_log(id,user_id,file_id,action,metadata)
         VALUES($1,$2,$3,'vectors_deleted',$4)",
    )
    .bind(Uuid::now_v7())
    .bind(user_id)
    .bind(file_id)
    .bind(serde_json::json!({
        "reason": reason,
        "deleted_references": deleted_refs,
        "deleted_legacy_vectors": legacy_deleted
    }))
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(deleted_refs + legacy_deleted)
}

pub async fn pending_deletions(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Vec<PendingDeletion>, String> {
    sqlx::query_as::<_, PendingDeletion>(
        "SELECT embedding_vector_id,user_id,collection_name,qdrant_point_id
         FROM embedding_vector_deletions
         WHERE user_id=$1
         ORDER BY created_at
         LIMIT 1000",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())
}

pub async fn pending_deletion_owners(pool: &PgPool) -> Result<Vec<Uuid>, String> {
    sqlx::query_scalar::<_, Uuid>(
        "SELECT DISTINCT user_id
         FROM embedding_vector_deletions
         ORDER BY user_id
         LIMIT 1000",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())
}

pub async fn unsynced_vectors(
    pool: &PgPool,
    embedding_model: &str,
    embedding_version: &str,
    limit: i64,
) -> Result<Vec<CanonicalVector>, String> {
    sqlx::query_as::<_, CanonicalVector>(&format!(
        "SELECT {VECTOR_COLUMNS}
         FROM embedding_vectors
         WHERE status='ready'
           AND qdrant_synced=false
           AND embedding_model=$1
           AND embedding_version=$2
         ORDER BY created_at
         LIMIT $3"
    ))
    .bind(embedding_model)
    .bind(embedding_version)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())
}

pub async fn record_deletion_failure(
    pool: &PgPool,
    ids: &[Uuid],
    error: &str,
) -> Result<(), String> {
    sqlx::query(
        "UPDATE embedding_vector_deletions
         SET attempts=attempts+1,last_error=$1,updated_at=now()
         WHERE embedding_vector_id = ANY($2)",
    )
    .bind(error)
    .bind(ids)
    .execute(pool)
    .await
    .map_err(|db_error| db_error.to_string())?;
    Ok(())
}

pub async fn finalize_deletions(pool: &PgPool, ids: &[Uuid]) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    let deleted = sqlx::query(
        "DELETE FROM embedding_vectors AS vector
         WHERE vector.id = ANY($1)
           AND vector.status='deleting'
           AND NOT EXISTS(
               SELECT 1 FROM embedding_chunk_refs AS ref
               WHERE ref.embedding_vector_id=vector.id
           )",
    )
    .bind(ids)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?
    .rows_affected();
    if deleted != ids.len() as u64 {
        return Err("canonical vector gained a reference while deletion was pending".into());
    }
    tx.commit().await.map_err(|error| error.to_string())
}

pub async fn search_references(
    pool: &PgPool,
    user_id: Uuid,
    vector_ids: &[Uuid],
    file_id: Option<Uuid>,
) -> Result<Vec<SearchReference>, String> {
    sqlx::query_as::<_, SearchReference>(
        "SELECT
            ref.embedding_vector_id,
            ref.file_id,
            ref.document_id,
            ref.chunk_id,
            ref.chunk_index,
            vector.content,
            vector.content_hash,
            vector.embedding_model,
            vector.embedding_version
         FROM embedding_chunk_refs AS ref
         JOIN embedding_vectors AS vector
          ON vector.id=ref.embedding_vector_id
          AND vector.user_id=ref.user_id
          AND vector.status='ready'
          AND vector.qdrant_synced=true
         JOIN files AS file
           ON file.id=ref.file_id
          AND file.user_id=ref.user_id
          AND file.status <> 'deleted'
         WHERE ref.user_id=$1
           AND ref.embedding_vector_id = ANY($2)
           AND ($3::UUID IS NULL OR ref.file_id=$3)
         ORDER BY ref.file_id,ref.chunk_index",
    )
    .bind(user_id)
    .bind(vector_ids)
    .bind(file_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())
}

pub async fn point_ids_for_file(
    pool: &PgPool,
    user_id: Uuid,
    file_id: Uuid,
    maximum: i64,
) -> Result<Vec<Uuid>, String> {
    let ids = sqlx::query_scalar::<_, Uuid>(
        "SELECT DISTINCT vector.qdrant_point_id
         FROM embedding_chunk_refs AS ref
         JOIN embedding_vectors AS vector
         ON vector.id=ref.embedding_vector_id
          AND vector.user_id=ref.user_id
          AND vector.qdrant_synced=true
          AND vector.status='ready'
         WHERE ref.user_id=$1 AND ref.file_id=$2
         LIMIT $3",
    )
    .bind(user_id)
    .bind(file_id)
    .bind(maximum + 1)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    if ids.len() as i64 > maximum {
        return Err("file contains too many canonical vectors for a filtered search".into());
    }
    Ok(ids)
}

async fn refresh_reference_counts(
    tx: &mut Transaction<'_, Postgres>,
    vector_ids: &[Uuid],
) -> Result<(), String> {
    if vector_ids.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "UPDATE embedding_vectors AS vector
         SET reference_count=(
                SELECT count(*)
                FROM embedding_chunk_refs AS ref
                WHERE ref.embedding_vector_id=vector.id
             ),
             updated_at=now()
         WHERE vector.id = ANY($1)",
    )
    .bind(vector_ids)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn lock_owned_file(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    file_id: Uuid,
) -> Result<(), String> {
    let locked =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM files WHERE id=$1 AND user_id=$2 FOR UPDATE")
            .bind(file_id)
            .bind(user_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(|error| error.to_string())?;
    if locked.is_none() {
        return Err("owned file disappeared while updating canonical references".into());
    }
    Ok(())
}

async fn queue_orphans(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    vector_ids: &[Uuid],
) -> Result<(), String> {
    if vector_ids.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO embedding_vector_deletions(
            embedding_vector_id,user_id,collection_name,qdrant_point_id
         )
         SELECT id,user_id,collection_name,qdrant_point_id
         FROM embedding_vectors
         WHERE user_id=$1
           AND id = ANY($2)
           AND status='ready'
           AND reference_count=0
         ON CONFLICT(embedding_vector_id) DO NOTHING",
    )
    .bind(user_id)
    .bind(vector_ids)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query(
        "UPDATE embedding_vectors
         SET status='deleting',updated_at=now()
         WHERE user_id=$1
           AND id = ANY($2)
           AND status='ready'
           AND reference_count=0",
    )
    .bind(user_id)
    .bind(vector_ids)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn refresh_vector_usage(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO vector_usage(
            user_id,vector_count,vector_bytes,document_count,updated_at
         )
         SELECT
            $1,
            (
                SELECT count(*)
                FROM embedding_vectors AS vector
                WHERE vector.user_id=$1
                  AND vector.status='ready'
                  AND vector.reference_count > 0
            ),
            (
                SELECT COALESCE(sum(vector.vector_bytes),0)
                FROM embedding_vectors AS vector
                WHERE vector.user_id=$1
                  AND vector.status='ready'
                  AND vector.reference_count > 0
            ),
            (
                SELECT count(DISTINCT ref.document_id)
                FROM embedding_chunk_refs AS ref
                WHERE ref.user_id=$1
            ),
            now()
         ON CONFLICT(user_id) DO UPDATE SET
            vector_count=EXCLUDED.vector_count,
            vector_bytes=EXCLUDED.vector_bytes,
            document_count=EXCLUDED.document_count,
            updated_at=now()",
    )
    .bind(user_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        commit_file_references, complete_owned_vectors, finalize_deletions, mark_qdrant_synced,
        remove_file_references, reserve_vectors, ChunkReference, FileReferenceCommit,
        IngestionAccounting, ReferenceCommitStats, ReserveVectorsInput, UniqueChunk,
    };
    use sha2::{Digest, Sha256};
    use sqlx::PgPool;
    use uuid::Uuid;

    #[tokio::test]
    #[ignore = "requires migrated FILE_SERVICE_DATABASE_URL"]
    async fn canonical_reservation_deduplicates_per_user_without_cross_user_reuse() {
        dotenv::dotenv().ok();
        let database_url = std::env::var("FILE_SERVICE_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .expect("FILE_SERVICE_DATABASE_URL or DATABASE_URL must be set");
        let pool = PgPool::connect(&database_url)
            .await
            .expect("failed to connect to file-service test database");
        let user_a = Uuid::now_v7();
        let user_b = Uuid::now_v7();
        let content = "canonical content shared only inside one user".to_string();
        let content_hash = hex::encode(Sha256::digest(content.as_bytes()));
        let chunks = vec![UniqueChunk {
            content_hash,
            content,
            token_count: 10,
        }];

        let (first, second) = tokio::join!(
            reserve_vectors(ReserveVectorsInput {
                pool: &pool,
                user_id: user_a,
                embedding_model: "qwen/test",
                embedding_version: "test-v1",
                strategy_family: "test-family",
                collection_name: "canonical_test",
                chunks: &chunks,
                lease_seconds: 60,
            }),
            reserve_vectors(ReserveVectorsInput {
                pool: &pool,
                user_id: user_a,
                embedding_model: "qwen/test",
                embedding_version: "test-v1",
                strategy_family: "test-family",
                collection_name: "canonical_test",
                chunks: &chunks,
                lease_seconds: 60,
            })
        );
        let first = match (first, second) {
            (Ok(winner), Err(loser)) | (Err(loser), Ok(winner)) => {
                assert!(loser.contains("another worker"));
                winner
            }
            (left, right) => panic!("expected one owner and one loser: {left:?} / {right:?}"),
        };
        assert_eq!(
            first.owned.len(),
            1,
            "only one worker may call the provider"
        );

        complete_owned_vectors(
            &pool,
            first.worker_id,
            &[(first.owned[0].id, vec![0.1, 0.2, 0.3, 0.4])],
        )
        .await
        .expect("first worker should complete the vector");

        let cache_hit = reserve_vectors(ReserveVectorsInput {
            pool: &pool,
            user_id: user_a,
            embedding_model: "qwen/test",
            embedding_version: "test-v1",
            strategy_family: "test-family",
            collection_name: "canonical_test",
            chunks: &chunks,
            lease_seconds: 60,
        })
        .await
        .expect("same user should reuse the completed vector");
        assert!(cache_hit.owned.is_empty());

        let isolated = reserve_vectors(ReserveVectorsInput {
            pool: &pool,
            user_id: user_b,
            embedding_model: "qwen/test",
            embedding_version: "test-v1",
            strategy_family: "test-family",
            collection_name: "canonical_test",
            chunks: &chunks,
            lease_seconds: 60,
        })
        .await
        .expect("another user must receive an independent reservation");
        assert_eq!(isolated.owned.len(), 1);
        assert_ne!(first.owned[0].id, isolated.owned[0].id);

        sqlx::query("DELETE FROM embedding_vectors WHERE user_id = ANY($1)")
            .bind(vec![user_a, user_b])
            .execute(&pool)
            .await
            .expect("failed to remove canonical integration-test rows");
    }

    #[tokio::test]
    #[ignore = "requires migrated FILE_SERVICE_DATABASE_URL"]
    async fn canonical_vector_is_deleted_only_after_its_last_file_reference() {
        dotenv::dotenv().ok();
        let database_url = std::env::var("FILE_SERVICE_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .expect("FILE_SERVICE_DATABASE_URL or DATABASE_URL must be set");
        let pool = PgPool::connect(&database_url)
            .await
            .expect("failed to connect to file-service test database");
        let user = Uuid::now_v7();
        let file_a = Uuid::now_v7();
        let file_b = Uuid::now_v7();
        for (file, suffix) in [(file_a, "a"), (file_b, "b")] {
            sqlx::query(
                "INSERT INTO files(
                    id,user_id,name,mime_type,size_bytes,sha256,status,object_key,
                    idempotency_key
                 )
                 VALUES($1,$2,$3,'text/plain',10,$4,'ready',$5,$6)",
            )
            .bind(file)
            .bind(user)
            .bind(format!("{suffix}.txt"))
            .bind("0".repeat(64))
            .bind(format!("canonical-test/{user}/{suffix}"))
            .bind(format!("canonical-test-{suffix}"))
            .execute(&pool)
            .await
            .expect("failed to insert test file");
        }

        let content = "one canonical vector referenced by two files".to_string();
        let content_hash = hex::encode(Sha256::digest(content.as_bytes()));
        let chunks = vec![UniqueChunk {
            content_hash,
            content,
            token_count: 10,
        }];
        let reservation = reserve_vectors(ReserveVectorsInput {
            pool: &pool,
            user_id: user,
            embedding_model: "qwen/test",
            embedding_version: "test-ref-v1",
            strategy_family: "test-family",
            collection_name: "canonical_test",
            chunks: &chunks,
            lease_seconds: 60,
        })
        .await
        .expect("failed to reserve canonical test vector");
        let vector_id = reservation.owned[0].id;
        complete_owned_vectors(
            &pool,
            reservation.worker_id,
            &[(vector_id, vec![0.1, 0.2, 0.3, 0.4])],
        )
        .await
        .expect("failed to complete canonical test vector");
        mark_qdrant_synced(&pool, user, "canonical_test", &[vector_id])
            .await
            .expect("failed to mark canonical test vector synchronized");

        for (file, index) in [(file_a, 0), (file_b, 1)] {
            let job_id = Uuid::now_v7();
            sqlx::query(
                "INSERT INTO ingestion_jobs(
                    id,file_id,user_id,status,strategy_version,embedding_model
                 )
                 VALUES($1,$2,$3,'processing','test-strategy','qwen/test')",
            )
            .bind(job_id)
            .bind(file)
            .bind(user)
            .execute(&pool)
            .await
            .expect("failed to insert ingestion test job");
            commit_file_references(
                &pool,
                &[ChunkReference {
                    chunk_id: Uuid::now_v7(),
                    chunk_index: index,
                    vector_id,
                }],
                FileReferenceCommit {
                    user_id: user,
                    file_id: file,
                    strategy_version: "test-strategy",
                    embedding_model: "qwen/test",
                    embedding_version: "test-ref-v1",
                    job_id,
                    accounting: IngestionAccounting {
                        embedding_count: 1,
                        input_tokens: 0,
                        cost_micros: 0,
                        cache_hits: if index > 0 { 1 } else { 0 },
                        cache_misses: if index == 0 { 1 } else { 0 },
                        tokens_avoided: 0,
                        cost_avoided_micros: 0,
                    },
                },
            )
            .await
            .expect("failed to commit canonical file reference");
        }

        let reference_count: i64 =
            sqlx::query_scalar("SELECT reference_count FROM embedding_vectors WHERE id=$1")
                .bind(vector_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(reference_count, 2);

        remove_file_references(&pool, user, file_a, "test")
            .await
            .expect("failed to remove first file reference");
        let after_first: (String, i64) =
            sqlx::query_as("SELECT status,reference_count FROM embedding_vectors WHERE id=$1")
                .bind(vector_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(after_first, ("ready".to_string(), 1));

        remove_file_references(&pool, user, file_b, "test")
            .await
            .expect("failed to remove last file reference");
        let after_last: (String, i64) =
            sqlx::query_as("SELECT status,reference_count FROM embedding_vectors WHERE id=$1")
                .bind(vector_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(after_last, ("deleting".to_string(), 0));

        finalize_deletions(&pool, &[vector_id])
            .await
            .expect("failed to finalize canonical vector deletion");
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM embedding_vectors WHERE id=$1)")
                .bind(vector_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(!exists);

        sqlx::query("DELETE FROM ingestion_jobs WHERE user_id=$1")
            .bind(user)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM files WHERE user_id=$1")
            .bind(user)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM file_audit_log WHERE user_id=$1")
            .bind(user)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore = "requires migrated FILE_SERVICE_DATABASE_URL"]
    async fn incremental_reference_commit_retains_unchanged_chunks() {
        dotenv::dotenv().ok();
        let database_url = std::env::var("FILE_SERVICE_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .expect("FILE_SERVICE_DATABASE_URL or DATABASE_URL must be set");
        let pool = PgPool::connect(&database_url)
            .await
            .expect("failed to connect to file-service test database");
        let user = Uuid::now_v7();
        let file = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO files(
                id,user_id,name,mime_type,size_bytes,sha256,status,object_key,idempotency_key
             ) VALUES($1,$2,'incremental.txt','text/plain',10,$3,'ready',$4,$5)",
        )
        .bind(file)
        .bind(user)
        .bind("0".repeat(64))
        .bind(format!("incremental-test/{user}"))
        .bind(format!("incremental-test-{user}"))
        .execute(&pool)
        .await
        .expect("failed to insert incremental test file");

        let contents = ["first chunk", "second chunk"];
        let chunks = contents
            .iter()
            .map(|content| UniqueChunk {
                content_hash: hex::encode(Sha256::digest(content.as_bytes())),
                content: (*content).to_string(),
                token_count: 2,
            })
            .collect::<Vec<_>>();
        let reservation = reserve_vectors(ReserveVectorsInput {
            pool: &pool,
            user_id: user,
            embedding_model: "qwen/test",
            embedding_version: "incremental-v1",
            strategy_family: "test-family",
            collection_name: "canonical_test",
            chunks: &chunks,
            lease_seconds: 60,
        })
        .await
        .expect("failed to reserve incremental vectors");
        let vector_ids = reservation
            .owned
            .iter()
            .map(|vector| vector.id)
            .collect::<Vec<_>>();
        complete_owned_vectors(
            &pool,
            reservation.worker_id,
            &[
                (vector_ids[0], vec![0.1, 0.2]),
                (vector_ids[1], vec![0.3, 0.4]),
            ],
        )
        .await
        .expect("failed to complete incremental vectors");
        mark_qdrant_synced(&pool, user, "canonical_test", &vector_ids)
            .await
            .expect("failed to synchronize incremental vectors");

        let job_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO ingestion_jobs(
                id,file_id,user_id,status,strategy_version,embedding_model
             ) VALUES($1,$2,$3,'processing','incremental-strategy','qwen/test')",
        )
        .bind(job_id)
        .bind(file)
        .bind(user)
        .execute(&pool)
        .await
        .expect("failed to insert incremental test job");
        let first = commit_file_references(
            &pool,
            &[
                ChunkReference {
                    chunk_id: Uuid::new_v5(&file, b"incremental-v1:0"),
                    chunk_index: 0,
                    vector_id: vector_ids[0],
                },
                ChunkReference {
                    chunk_id: Uuid::new_v5(&file, b"incremental-v1:1"),
                    chunk_index: 1,
                    vector_id: vector_ids[1],
                },
            ],
            FileReferenceCommit {
                user_id: user,
                file_id: file,
                strategy_version: "incremental-strategy",
                embedding_model: "qwen/test",
                embedding_version: "incremental-v1",
                job_id,
                accounting: IngestionAccounting {
                    embedding_count: 2,
                    input_tokens: 4,
                    cost_micros: 1,
                    cache_hits: 0,
                    cache_misses: 2,
                    tokens_avoided: 0,
                    cost_avoided_micros: 0,
                },
            },
        )
        .await
        .expect("failed to commit initial references");
        assert_eq!(
            first,
            ReferenceCommitStats {
                retained: 0,
                added: 2,
                removed: 0
            }
        );
        let original_reference_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM embedding_chunk_refs WHERE file_id=$1 AND chunk_index=0",
        )
        .bind(file)
        .fetch_one(&pool)
        .await
        .expect("missing initial reference");

        sqlx::query("UPDATE ingestion_jobs SET status='processing' WHERE id=$1")
            .bind(job_id)
            .execute(&pool)
            .await
            .expect("failed to reopen incremental test job");
        let second = commit_file_references(
            &pool,
            &[ChunkReference {
                chunk_id: Uuid::new_v5(&file, b"incremental-v1:0"),
                chunk_index: 0,
                vector_id: vector_ids[0],
            }],
            FileReferenceCommit {
                user_id: user,
                file_id: file,
                strategy_version: "incremental-strategy",
                embedding_model: "qwen/test",
                embedding_version: "incremental-v1",
                job_id,
                accounting: IngestionAccounting {
                    embedding_count: 1,
                    input_tokens: 2,
                    cost_micros: 1,
                    cache_hits: 1,
                    cache_misses: 0,
                    tokens_avoided: 2,
                    cost_avoided_micros: 1,
                },
            },
        )
        .await
        .expect("failed to commit incremental references");
        assert_eq!(
            second,
            ReferenceCommitStats {
                retained: 1,
                added: 0,
                removed: 1
            }
        );
        let retained_reference_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM embedding_chunk_refs WHERE file_id=$1 AND chunk_index=0",
        )
        .bind(file)
        .fetch_one(&pool)
        .await
        .expect("missing retained reference");
        assert_eq!(retained_reference_id, original_reference_id);

        remove_file_references(&pool, user, file, "incremental-test")
            .await
            .expect("failed to remove incremental test references");
        sqlx::query("DELETE FROM ingestion_jobs WHERE user_id=$1")
            .bind(user)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM files WHERE user_id=$1")
            .bind(user)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM embedding_vectors WHERE user_id=$1")
            .bind(user)
            .execute(&pool)
            .await
            .unwrap();
    }
}
