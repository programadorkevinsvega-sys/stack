ALTER TABLE subscriptions
    ADD COLUMN IF NOT EXISTS trial_ends_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS grace_period_ends_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS payment_retry_count INT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS last_payment_error TEXT,
    ADD COLUMN IF NOT EXISTS provider_subscription_id VARCHAR(255);
CREATE INDEX IF NOT EXISTS idx_subscriptions_billing_due ON subscriptions(status, current_period_end);
