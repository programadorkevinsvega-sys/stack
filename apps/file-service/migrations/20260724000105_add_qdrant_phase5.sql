ALTER TABLE embedding_chunks
    ADD COLUMN IF NOT EXISTS document_id UUID,
    ADD COLUMN IF NOT EXISTS chunk_id UUID,
    ADD COLUMN IF NOT EXISTS embedding_version VARCHAR(64),
    ADD COLUMN IF NOT EXISTS vector_bytes BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS collection_name VARCHAR(255);

UPDATE embedding_chunks
SET document_id = COALESCE(document_id, file_id),
    chunk_id = COALESCE(chunk_id, id),
    embedding_version = COALESCE(embedding_version, 'legacy-v1'),
    vector_bytes = CASE
        WHEN vector_bytes > 0 THEN vector_bytes
        WHEN jsonb_typeof(embedding) = 'array' THEN jsonb_array_length(embedding) * 4
        ELSE 0
    END,
    collection_name = COALESCE(collection_name, 'nexus_embeddings');

ALTER TABLE embedding_chunks
    ALTER COLUMN document_id SET NOT NULL,
    ALTER COLUMN chunk_id SET NOT NULL,
    ALTER COLUMN embedding_version SET NOT NULL,
    ALTER COLUMN collection_name SET NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS uq_embedding_chunks_chunk_id
    ON embedding_chunks(chunk_id);
CREATE INDEX IF NOT EXISTS idx_embedding_chunks_owner_document
    ON embedding_chunks(user_id, document_id);

CREATE TABLE IF NOT EXISTS vector_usage (
    user_id UUID PRIMARY KEY,
    vector_count BIGINT NOT NULL DEFAULT 0 CHECK (vector_count >= 0),
    vector_bytes BIGINT NOT NULL DEFAULT 0 CHECK (vector_bytes >= 0),
    document_count BIGINT NOT NULL DEFAULT 0 CHECK (document_count >= 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO vector_usage(user_id, vector_count, vector_bytes, document_count)
SELECT user_id, count(*), COALESCE(sum(vector_bytes), 0), count(DISTINCT document_id)
FROM embedding_chunks
GROUP BY user_id
ON CONFLICT(user_id) DO UPDATE SET
    vector_count = EXCLUDED.vector_count,
    vector_bytes = EXCLUDED.vector_bytes,
    document_count = EXCLUDED.document_count,
    updated_at = now();
