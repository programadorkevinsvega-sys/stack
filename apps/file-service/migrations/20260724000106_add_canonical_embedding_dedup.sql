ALTER TABLE ingestion_jobs
    ADD COLUMN IF NOT EXISTS cache_hits BIGINT NOT NULL DEFAULT 0
        CHECK (cache_hits >= 0),
    ADD COLUMN IF NOT EXISTS cache_misses BIGINT NOT NULL DEFAULT 0
        CHECK (cache_misses >= 0),
    ADD COLUMN IF NOT EXISTS tokens_avoided BIGINT NOT NULL DEFAULT 0
        CHECK (tokens_avoided >= 0),
    ADD COLUMN IF NOT EXISTS cost_avoided_micros BIGINT NOT NULL DEFAULT 0
        CHECK (cost_avoided_micros >= 0);

CREATE TABLE IF NOT EXISTS embedding_vectors (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL,
    content_hash CHAR(64) NOT NULL,
    content TEXT NOT NULL,
    embedding_model VARCHAR(255) NOT NULL,
    embedding_version VARCHAR(64) NOT NULL,
    embedding JSONB,
    token_count INT NOT NULL DEFAULT 0 CHECK (token_count >= 0),
    vector_dimension INT CHECK (vector_dimension > 0),
    vector_bytes BIGINT NOT NULL DEFAULT 0 CHECK (vector_bytes >= 0),
    collection_name VARCHAR(255) NOT NULL,
    qdrant_point_id UUID NOT NULL UNIQUE,
    qdrant_synced BOOLEAN NOT NULL DEFAULT false,
    status VARCHAR(20) NOT NULL
        CHECK (status IN ('reserved','embedding','ready','failed','deleting')),
    lease_owner UUID,
    lease_until TIMESTAMPTZ,
    reference_count BIGINT NOT NULL DEFAULT 0 CHECK (reference_count >= 0),
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (
        (status = 'ready' AND embedding IS NOT NULL
            AND vector_dimension IS NOT NULL AND vector_bytes > 0)
        OR status <> 'ready'
    ),
    UNIQUE(user_id, content_hash, embedding_model, embedding_version)
);

CREATE INDEX IF NOT EXISTS idx_embedding_vectors_owner_status
    ON embedding_vectors(user_id, status, updated_at);
CREATE INDEX IF NOT EXISTS idx_embedding_vectors_expired_lease
    ON embedding_vectors(status, lease_until)
    WHERE status IN ('reserved','embedding');
CREATE INDEX IF NOT EXISTS idx_embedding_vectors_orphans
    ON embedding_vectors(user_id, updated_at)
    WHERE status = 'ready' AND reference_count = 0;

CREATE TABLE IF NOT EXISTS embedding_chunk_refs (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL,
    file_id UUID NOT NULL REFERENCES files(id),
    document_id UUID NOT NULL,
    chunk_id UUID NOT NULL UNIQUE,
    chunk_index INT NOT NULL CHECK (chunk_index >= 0),
    strategy_version VARCHAR(64) NOT NULL,
    embedding_model VARCHAR(255) NOT NULL,
    embedding_version VARCHAR(64) NOT NULL,
    embedding_vector_id UUID NOT NULL REFERENCES embedding_vectors(id)
        ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(file_id, strategy_version, embedding_model, embedding_version, chunk_index)
);

CREATE INDEX IF NOT EXISTS idx_embedding_chunk_refs_owner_file
    ON embedding_chunk_refs(user_id, file_id, chunk_index);
CREATE INDEX IF NOT EXISTS idx_embedding_chunk_refs_vector
    ON embedding_chunk_refs(embedding_vector_id);
CREATE INDEX IF NOT EXISTS idx_embedding_chunk_refs_owner_document
    ON embedding_chunk_refs(user_id, document_id);

CREATE TABLE IF NOT EXISTS embedding_vector_deletions (
    embedding_vector_id UUID PRIMARY KEY REFERENCES embedding_vectors(id)
        ON DELETE CASCADE,
    user_id UUID NOT NULL,
    collection_name VARCHAR(255) NOT NULL,
    qdrant_point_id UUID NOT NULL,
    attempts INT NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_embedding_vector_deletions_owner
    ON embedding_vector_deletions(user_id, created_at);

-- Backfill the canonical cache from the legacy per-chunk table without
-- collapsing data across users, models, or embedding versions.
INSERT INTO embedding_vectors (
    id, user_id, content_hash, content, embedding_model, embedding_version,
    embedding, token_count, vector_dimension, vector_bytes, collection_name,
    qdrant_point_id, qdrant_synced, status, reference_count, created_at, updated_at
)
SELECT DISTINCT ON (
    chunk.user_id,
    chunk.content_hash,
    chunk.embedding_model,
    chunk.embedding_version
)
    chunk.id,
    chunk.user_id,
    chunk.content_hash,
    chunk.content,
    chunk.embedding_model,
    chunk.embedding_version,
    chunk.embedding,
    chunk.token_count,
    jsonb_array_length(chunk.embedding),
    chunk.vector_bytes,
    CASE
        WHEN chunk.collection_name = 'nexus_embeddings'
            THEN 'nexus_embeddings_v2'
        WHEN chunk.collection_name LIKE 'nexus_embeddings_%'
            THEN 'nexus_embeddings_v2_' ||
                substring(chunk.collection_name FROM length('nexus_embeddings_') + 1)
        WHEN right(chunk.collection_name, 3) = '_v2' THEN chunk.collection_name
        ELSE left(chunk.collection_name, 252) || '_v2'
    END,
    chunk.id,
    false,
    'ready',
    0,
    chunk.created_at,
    now()
FROM embedding_chunks AS chunk
WHERE jsonb_typeof(chunk.embedding) = 'array'
  AND jsonb_array_length(chunk.embedding) > 0
ORDER BY
    chunk.user_id,
    chunk.content_hash,
    chunk.embedding_model,
    chunk.embedding_version,
    chunk.created_at,
    chunk.id
ON CONFLICT(user_id, content_hash, embedding_model, embedding_version)
DO NOTHING;

INSERT INTO embedding_chunk_refs (
    id, user_id, file_id, document_id, chunk_id, chunk_index,
    strategy_version, embedding_model, embedding_version, embedding_vector_id,
    created_at
)
SELECT
    chunk.id,
    chunk.user_id,
    chunk.file_id,
    chunk.document_id,
    chunk.chunk_id,
    chunk.chunk_index,
    chunk.strategy_version,
    chunk.embedding_model,
    chunk.embedding_version,
    vector.id,
    chunk.created_at
FROM embedding_chunks AS chunk
JOIN embedding_vectors AS vector
  ON vector.user_id = chunk.user_id
 AND vector.content_hash = chunk.content_hash
 AND vector.embedding_model = chunk.embedding_model
 AND vector.embedding_version = chunk.embedding_version
ON CONFLICT DO NOTHING;

UPDATE embedding_vectors AS vector
SET reference_count = refs.total,
    updated_at = now()
FROM (
    SELECT embedding_vector_id, count(*)::BIGINT AS total
    FROM embedding_chunk_refs
    GROUP BY embedding_vector_id
) AS refs
WHERE vector.id = refs.embedding_vector_id;

INSERT INTO vector_usage(
    user_id, vector_count, vector_bytes, document_count, updated_at
)
SELECT
    owner.user_id,
    (
        SELECT count(*)
        FROM embedding_vectors AS vector
        WHERE vector.user_id = owner.user_id
          AND vector.status = 'ready'
          AND vector.reference_count > 0
    ),
    (
        SELECT COALESCE(sum(vector.vector_bytes), 0)
        FROM embedding_vectors AS vector
        WHERE vector.user_id = owner.user_id
          AND vector.status = 'ready'
          AND vector.reference_count > 0
    ),
    count(DISTINCT owner.document_id),
    now()
FROM embedding_chunk_refs AS owner
GROUP BY owner.user_id
ON CONFLICT(user_id) DO UPDATE SET
    vector_count = EXCLUDED.vector_count,
    vector_bytes = EXCLUDED.vector_bytes,
    document_count = EXCLUDED.document_count,
    updated_at = now();
