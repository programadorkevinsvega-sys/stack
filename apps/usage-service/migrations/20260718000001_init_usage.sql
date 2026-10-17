
CREATE TABLE IF NOT EXISTS usage_events (
    id UUID NOT NULL DEFAULT gen_random_uuid(),
    request_id UUID NOT NULL,
    api_key_id UUID NOT NULL,
    user_id UUID NOT NULL,
    model_catalog_id UUID NOT NULL,
    aws_account_id UUID NOT NULL,
    input_tokens INT NOT NULL CHECK (input_tokens >= 0),
    output_tokens INT NOT NULL CHECK (output_tokens >= 0),
    cached_tokens INT NOT NULL DEFAULT 0 CHECK (cached_tokens >= 0),
    stream BOOLEAN NOT NULL DEFAULT false,
    latency_ms INT NOT NULL CHECK (latency_ms >= 0),
    ttft_ms INT CHECK (ttft_ms IS NULL OR ttft_ms >= 0),
    status_code SMALLINT NOT NULL,
    error_code VARCHAR(50),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (id, created_at)
) PARTITION BY RANGE (created_at);

CREATE TABLE IF NOT EXISTS usage_events_2026_07 PARTITION OF usage_events
FOR VALUES FROM ('2026-07-01') TO ('2026-08-01');

CREATE TABLE IF NOT EXISTS usage_events_2026_08 PARTITION OF usage_events
FOR VALUES FROM ('2026-08-01') TO ('2026-09-01');

CREATE TABLE IF NOT EXISTS usage_events_2026_09 PARTITION OF usage_events
FOR VALUES FROM ('2026-09-01') TO ('2026-10-01');

CREATE INDEX IF NOT EXISTS idx_usage_events_apikey_time
    ON usage_events(api_key_id, created_at);

CREATE INDEX IF NOT EXISTS idx_usage_events_user_time
    ON usage_events(user_id, created_at);

CREATE INDEX IF NOT EXISTS idx_usage_events_model_time
    ON usage_events(model_catalog_id, created_at);

CREATE UNIQUE INDEX IF NOT EXISTS idx_usage_events_request_id
    ON usage_events(request_id, created_at);

CREATE MATERIALIZED VIEW IF NOT EXISTS usage_hourly AS
SELECT
    api_key_id,
    user_id,
    model_catalog_id,
    date_trunc('hour', created_at) AS hour,
    SUM(input_tokens) AS total_input_tokens,
    SUM(output_tokens) AS total_output_tokens,
    COUNT(*) AS request_count,
    COUNT(*) FILTER (WHERE status_code >= 400) AS error_count,
    AVG(latency_ms) AS avg_latency_ms
FROM usage_events
GROUP BY api_key_id, user_id, model_catalog_id, date_trunc('hour', created_at)
WITH NO DATA;

CREATE UNIQUE INDEX IF NOT EXISTS idx_usage_hourly_pk
    ON usage_hourly(api_key_id, model_catalog_id, hour);

CREATE MATERIALIZED VIEW IF NOT EXISTS usage_daily AS
SELECT
    user_id,
    model_catalog_id,
    date_trunc('day', created_at) AS day,
    SUM(input_tokens) AS total_input_tokens,
    SUM(output_tokens) AS total_output_tokens,
    COUNT(*) AS request_count
FROM usage_events
GROUP BY user_id, model_catalog_id, date_trunc('day', created_at)
WITH NO DATA;

CREATE UNIQUE INDEX IF NOT EXISTS idx_usage_daily_pk
    ON usage_daily(user_id, model_catalog_id, day);
