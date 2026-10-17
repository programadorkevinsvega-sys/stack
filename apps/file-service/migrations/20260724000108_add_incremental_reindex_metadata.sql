-- Fase 4C: immutable ingestion telemetry and incremental reindex accounting.
-- The migration is additive so existing ingestion history remains readable.
ALTER TABLE ingestion_jobs
    ADD COLUMN IF NOT EXISTS source_sha256 CHAR(64),
    ADD COLUMN IF NOT EXISTS chunks_retained BIGINT NOT NULL DEFAULT 0
        CHECK (chunks_retained >= 0),
    ADD COLUMN IF NOT EXISTS chunks_added BIGINT NOT NULL DEFAULT 0
        CHECK (chunks_added >= 0),
    ADD COLUMN IF NOT EXISTS chunks_removed BIGINT NOT NULL DEFAULT 0
        CHECK (chunks_removed >= 0),
    ADD COLUMN IF NOT EXISTS started_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS completed_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_ingestion_jobs_file_history
    ON ingestion_jobs(file_id, strategy_version, embedding_model, updated_at DESC);

ALTER TABLE ingestion_jobs
    ADD CONSTRAINT ingestion_jobs_source_sha256_hex
    CHECK (source_sha256 IS NULL OR source_sha256 ~ '^[0-9a-f]{64}$');
