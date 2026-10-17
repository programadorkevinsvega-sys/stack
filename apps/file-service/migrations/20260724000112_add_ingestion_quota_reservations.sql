CREATE TABLE IF NOT EXISTS ingestion_quota_usage (
    user_id UUID PRIMARY KEY,
    hour_started_at TIMESTAMPTZ NOT NULL DEFAULT date_trunc('hour', now()),
    day_started_at TIMESTAMPTZ NOT NULL DEFAULT date_trunc('day', now()),
    month_started_at TIMESTAMPTZ NOT NULL DEFAULT date_trunc('month', now()),
    files_hour BIGINT NOT NULL DEFAULT 0 CHECK (files_hour >= 0),
    files_day BIGINT NOT NULL DEFAULT 0 CHECK (files_day >= 0),
    bytes_day BIGINT NOT NULL DEFAULT 0 CHECK (bytes_day >= 0),
    tokens_month BIGINT NOT NULL DEFAULT 0 CHECK (tokens_month >= 0),
    pending_jobs BIGINT NOT NULL DEFAULT 0 CHECK (pending_jobs >= 0),
    concurrent_jobs BIGINT NOT NULL DEFAULT 0 CHECK (concurrent_jobs >= 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS ingestion_quota_reservations (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL,
    file_id UUID NOT NULL REFERENCES files(id),
    status VARCHAR(20) NOT NULL CHECK (status IN ('reserved','processing','completed','released')),
    reserved_bytes BIGINT NOT NULL CHECK (reserved_bytes > 0),
    estimated_tokens BIGINT NOT NULL CHECK (estimated_tokens >= 0),
    actual_tokens BIGINT NOT NULL DEFAULT 0 CHECK (actual_tokens >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    last_error TEXT
);

CREATE INDEX IF NOT EXISTS idx_ingestion_quota_reservations_user_status
    ON ingestion_quota_reservations(user_id, status, created_at);
CREATE INDEX IF NOT EXISTS idx_ingestion_quota_reservations_file
    ON ingestion_quota_reservations(user_id, file_id, created_at DESC);
