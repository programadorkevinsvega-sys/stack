ALTER TABLE plans
    ADD COLUMN IF NOT EXISTS max_ingestion_files_per_hour BIGINT NOT NULL DEFAULT 0 CHECK (max_ingestion_files_per_hour >= 0),
    ADD COLUMN IF NOT EXISTS max_ingestion_files_per_day BIGINT NOT NULL DEFAULT 0 CHECK (max_ingestion_files_per_day >= 0),
    ADD COLUMN IF NOT EXISTS max_ingestion_bytes_per_day BIGINT NOT NULL DEFAULT 0 CHECK (max_ingestion_bytes_per_day >= 0),
    ADD COLUMN IF NOT EXISTS max_ingestion_tokens_per_month BIGINT NOT NULL DEFAULT 0 CHECK (max_ingestion_tokens_per_month >= 0),
    ADD COLUMN IF NOT EXISTS max_concurrent_ingestions BIGINT NOT NULL DEFAULT 1 CHECK (max_concurrent_ingestions > 0),
    ADD COLUMN IF NOT EXISTS max_pending_ingestion_jobs BIGINT NOT NULL DEFAULT 10 CHECK (max_pending_ingestion_jobs > 0);

UPDATE plans SET
    max_ingestion_files_per_hour = CASE slug WHEN 'starter' THEN 20 WHEN 'pro' THEN 200 WHEN 'pro-plus' THEN 1000 ELSE max_ingestion_files_per_hour END,
    max_ingestion_files_per_day = CASE slug WHEN 'starter' THEN 100 WHEN 'pro' THEN 2000 WHEN 'pro-plus' THEN 10000 ELSE max_ingestion_files_per_day END,
    max_ingestion_bytes_per_day = CASE slug WHEN 'starter' THEN 1073741824 WHEN 'pro' THEN 21474836480 WHEN 'pro-plus' THEN 107374182400 ELSE max_ingestion_bytes_per_day END,
    max_ingestion_tokens_per_month = CASE slug WHEN 'starter' THEN 100000 WHEN 'pro' THEN 10000000 WHEN 'pro-plus' THEN 0 ELSE max_ingestion_tokens_per_month END,
    max_concurrent_ingestions = CASE slug WHEN 'starter' THEN 1 WHEN 'pro' THEN 10 WHEN 'pro-plus' THEN 25 ELSE max_concurrent_ingestions END,
    max_pending_ingestion_jobs = CASE slug WHEN 'starter' THEN 10 WHEN 'pro' THEN 100 WHEN 'pro-plus' THEN 500 ELSE max_pending_ingestion_jobs END
WHERE slug IN ('starter','pro','pro-plus');
