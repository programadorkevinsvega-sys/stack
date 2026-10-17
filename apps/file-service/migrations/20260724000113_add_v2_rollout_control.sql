-- Fase 9A rollout bookkeeping. Runtime feature flags remain environment
-- controlled so operators can change read/write mode without a code deploy.
CREATE TABLE IF NOT EXISTS vector_migration_runs (
    id UUID PRIMARY KEY,
    operation VARCHAR(32) NOT NULL CHECK (operation IN ('backfill','compare','cleanup')),
    source_schema VARCHAR(32) NOT NULL,
    target_schema VARCHAR(32) NOT NULL,
    status VARCHAR(20) NOT NULL CHECK (status IN ('running','completed','failed','rolled_back')),
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    actor VARCHAR(128) NOT NULL,
    details JSONB NOT NULL DEFAULT '{}',
    last_error TEXT
);
CREATE INDEX IF NOT EXISTS idx_vector_migration_runs_operation
    ON vector_migration_runs(operation, started_at DESC);

CREATE TABLE IF NOT EXISTS vector_migration_reports (
    id UUID PRIMARY KEY,
    run_id UUID NOT NULL REFERENCES vector_migration_runs(id),
    user_id UUID,
    source_rows BIGINT NOT NULL DEFAULT 0,
    target_rows BIGINT NOT NULL DEFAULT 0,
    source_users BIGINT NOT NULL DEFAULT 0,
    target_users BIGINT NOT NULL DEFAULT 0,
    cross_user_mismatches BIGINT NOT NULL DEFAULT 0,
    hash_mismatches BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_vector_migration_reports_run
    ON vector_migration_reports(run_id, user_id);
