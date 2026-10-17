ALTER TABLE aws_account_models
    ADD COLUMN IF NOT EXISTS mantle_api_key_encrypted BYTEA,
    ADD COLUMN IF NOT EXISTS mantle_api_kms_key_id VARCHAR(255),
    ADD COLUMN IF NOT EXISTS mantle_base_url VARCHAR(2048),
    ADD COLUMN IF NOT EXISTS mantle_expires_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_aws_account_models_mantle
    ON aws_account_models(account_id, model_catalog_id)
    WHERE mantle_api_key_encrypted IS NOT NULL;
