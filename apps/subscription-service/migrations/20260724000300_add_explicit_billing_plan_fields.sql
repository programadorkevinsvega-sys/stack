ALTER TABLE plans
    ADD COLUMN IF NOT EXISTS price_amount NUMERIC(12,2) NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS currency VARCHAR(3) NOT NULL DEFAULT 'USD',
    ADD COLUMN IF NOT EXISTS trial_days INT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS max_api_keys BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS overage_enabled BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS fair_use_policy TEXT NOT NULL DEFAULT 'standard',
    ADD COLUMN IF NOT EXISTS grace_period_days INT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS sso_enabled BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS sla_enabled BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS mtls_enabled BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS s3_api_enabled BOOLEAN NOT NULL DEFAULT false;

ALTER TABLE plans
    ADD CONSTRAINT plans_currency_check CHECK (currency ~ '^[A-Z]{3}$'),
    ADD CONSTRAINT plans_price_check CHECK (price_amount >= 0),
    ADD CONSTRAINT plans_trial_days_check CHECK (trial_days >= 0),
    ADD CONSTRAINT plans_grace_period_check CHECK (grace_period_days >= 0);

UPDATE plans SET
    price_amount = COALESCE(NULLIF(features->>'price_usd', '')::numeric, 0),
    currency = 'USD',
    trial_days = COALESCE(NULLIF(features->>'trial_days', '')::int, 0),
    max_api_keys = COALESCE(NULLIF(features->>'max_api_keys', '')::bigint, 0),
    overage_enabled = COALESCE((features->>'overage')::boolean, false),
    fair_use_policy = CASE WHEN COALESCE((features->>'fair_use')::boolean, false) THEN 'fair_use' ELSE 'standard' END,
    sso_enabled = COALESCE((features->>'sso')::boolean, false),
    sla_enabled = COALESCE((features->>'sla')::boolean, false),
    mtls_enabled = COALESCE((features->>'mtls')::boolean, false),
    s3_api_enabled = COALESCE((features->>'s3_api')::boolean, false);

UPDATE plans SET
    price_amount = CASE slug WHEN 'starter' THEN 5 WHEN 'pro' THEN 10 WHEN 'pro-plus' THEN 20 ELSE price_amount END,
    currency = 'USD',
    trial_days = CASE WHEN slug = 'starter' THEN 3 ELSE 0 END,
    max_api_keys = CASE WHEN slug = 'starter' THEN 1 ELSE 0 END,
    overage_enabled = CASE WHEN slug IN ('pro', 'pro-plus') THEN true ELSE false END,
    fair_use_policy = CASE WHEN slug IN ('pro', 'pro-plus') THEN 'fair_use' ELSE 'standard' END,
    s3_api_enabled = CASE WHEN slug = 'pro-plus' THEN true ELSE s3_api_enabled END
WHERE slug IN ('starter', 'pro', 'pro-plus');
