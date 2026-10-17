CREATE TABLE IF NOT EXISTS terms_acceptances (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL,
    terms_version VARCHAR(64) NOT NULL,
    privacy_version VARCHAR(64) NOT NULL,
    accepted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    ip_address INET,
    user_agent TEXT NOT NULL DEFAULT '',
    country VARCHAR(2),
    UNIQUE (user_id, terms_version, privacy_version)
);
CREATE INDEX IF NOT EXISTS idx_terms_acceptances_user ON terms_acceptances(user_id, accepted_at DESC);
