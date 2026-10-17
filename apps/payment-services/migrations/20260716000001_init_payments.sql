
CREATE TABLE IF NOT EXISTS payment_methods (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL,
    provider VARCHAR(30) NOT NULL DEFAULT 'paypal',
    provider_payment_method_id VARCHAR(150) NOT NULL,
    brand VARCHAR(20),
    last4 CHAR(4),
    exp_month SMALLINT,
    exp_year SMALLINT,
    is_default BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS payment_provider_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider VARCHAR(30) NOT NULL,
    provider_event_id VARCHAR(150) NOT NULL UNIQUE,
    event_type VARCHAR(100) NOT NULL,
    payload JSONB NOT NULL,
    processed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS payment_transactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL,
    provider VARCHAR(30) NOT NULL,
    provider_reference VARCHAR(150) NOT NULL,
    kind VARCHAR(30) NOT NULL DEFAULT 'top_up'
        CHECK (kind IN ('top_up','subscription_charge','refund')),
    status VARCHAR(30) NOT NULL
        CHECK (status IN ('pending','requires_approval','succeeded','failed','refunded')),
    amount NUMERIC(18,6) NOT NULL,
    currency CHAR(3) NOT NULL,
    idempotency_key VARCHAR(100),
    failure_reason TEXT,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider, provider_reference),
    UNIQUE (provider, idempotency_key)
);

CREATE INDEX IF NOT EXISTS idx_payment_transactions_user
    ON payment_transactions(user_id, created_at DESC);
