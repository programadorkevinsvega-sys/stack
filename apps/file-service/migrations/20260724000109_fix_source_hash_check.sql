-- CHAR(64) is retained for compatibility with the existing schema, but its
-- blank padding must not invalidate a lowercase SHA-256 check.
ALTER TABLE ingestion_jobs
    DROP CONSTRAINT IF EXISTS ingestion_jobs_source_sha256_hex;

ALTER TABLE ingestion_jobs
    ADD CONSTRAINT ingestion_jobs_source_sha256_hex
    CHECK (source_sha256 IS NULL OR btrim(source_sha256) ~ '^[0-9a-f]{64}$');
