CREATE TABLE IF NOT EXISTS event_outbox (
    id UUID PRIMARY KEY,
    event_type VARCHAR(100) NOT NULL,
    payload JSONB NOT NULL CHECK (jsonb_typeof(payload) = 'object'),
    status VARCHAR(20) NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','processing','published','failed')),
    attempts INT NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    locked_at TIMESTAMPTZ,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    published_at TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_file_event_outbox_relay ON event_outbox(status, available_at, created_at) WHERE status IN ('pending','processing','failed');

CREATE TABLE IF NOT EXISTS ingestion_jobs (
    id UUID PRIMARY KEY,
    file_id UUID NOT NULL REFERENCES files(id),
    user_id UUID NOT NULL,
    status VARCHAR(20) NOT NULL CHECK (status IN ('pending','processing','completed','failed','dead_letter')),
    strategy_version VARCHAR(64) NOT NULL,
    embedding_model VARCHAR(255) NOT NULL,
    attempts INT NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    input_tokens BIGINT NOT NULL DEFAULT 0 CHECK (input_tokens >= 0),
    embedding_count INT NOT NULL DEFAULT 0 CHECK (embedding_count >= 0),
    cost_micros BIGINT NOT NULL DEFAULT 0 CHECK (cost_micros >= 0),
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(file_id, strategy_version, embedding_model)
);
CREATE INDEX IF NOT EXISTS idx_ingestion_jobs_user ON ingestion_jobs(user_id, status, created_at DESC);

CREATE TABLE IF NOT EXISTS embedding_chunks (
    id UUID PRIMARY KEY,
    file_id UUID NOT NULL REFERENCES files(id),
    user_id UUID NOT NULL,
    chunk_index INT NOT NULL CHECK (chunk_index >= 0),
    strategy_version VARCHAR(64) NOT NULL,
    embedding_model VARCHAR(255) NOT NULL,
    content TEXT NOT NULL,
    content_hash CHAR(64) NOT NULL,
    embedding JSONB NOT NULL,
    token_count INT NOT NULL DEFAULT 0 CHECK (token_count >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(file_id, strategy_version, embedding_model, chunk_index)
);
CREATE INDEX IF NOT EXISTS idx_embedding_chunks_user_file ON embedding_chunks(user_id, file_id);
