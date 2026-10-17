ALTER TABLE plans
    ADD COLUMN IF NOT EXISTS storage_bytes_quota BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS max_file_size_bytes BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS max_files BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS embeddings_enabled BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS rag_enabled BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS qdrant_mode VARCHAR(20) NOT NULL DEFAULT 'none';

ALTER TABLE plans
    ADD CONSTRAINT plans_qdrant_mode_check
    CHECK (qdrant_mode IN ('none', 'shared', 'dedicated'));
