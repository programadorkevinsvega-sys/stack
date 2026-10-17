-- Database: credentials_db (public)
CREATE TABLE IF NOT EXISTS aws_credential_pool (
    id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_alias        VARCHAR(100) NOT NULL,
    aws_account_id       VARCHAR(20) NOT NULL,
    region               VARCHAR(20) NOT NULL,
    auth_method          VARCHAR(20) NOT NULL DEFAULT 'sts_assume_role'
                             CHECK (auth_method IN ('sts_assume_role','static_keys')),
    role_arn             VARCHAR(255),
    external_id          VARCHAR(255),
    access_key_encrypted BYTEA,
    secret_key_encrypted BYTEA,
    kms_key_id           VARCHAR(255),
    max_tpm              INT NOT NULL,
    max_rpm              INT NOT NULL,
    priority_weight      SMALLINT NOT NULL DEFAULT 100,
    is_healthy           BOOLEAN NOT NULL DEFAULT true,
    consecutive_errors   SMALLINT NOT NULL DEFAULT 0,
    last_error_at        TIMESTAMPTZ,
    last_error_code      VARCHAR(50),
    last_health_check_at TIMESTAMPTZ,
    is_active            BOOLEAN NOT NULL DEFAULT true,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT chk_auth_method_fields CHECK (
        (auth_method = 'sts_assume_role' AND role_arn IS NOT NULL) OR
        (auth_method = 'static_keys' AND access_key_encrypted IS NOT NULL AND secret_key_encrypted IS NOT NULL)
    )
);

CREATE TABLE IF NOT EXISTS aws_account_models (
    account_id       UUID NOT NULL REFERENCES aws_credential_pool(id) ON DELETE CASCADE,
    model_catalog_id UUID NOT NULL,
    PRIMARY KEY (account_id, model_catalog_id)
);

CREATE INDEX IF NOT EXISTS idx_credential_pool_healthy
    ON aws_credential_pool(is_healthy, is_active);
