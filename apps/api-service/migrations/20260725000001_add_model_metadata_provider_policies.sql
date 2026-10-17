CREATE TABLE IF NOT EXISTS model_metadata (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    model_catalog_id    UUID NOT NULL UNIQUE
        REFERENCES model_catalog(id) ON DELETE CASCADE,
    description         TEXT,
    architecture        VARCHAR(100),
    parameter_count     BIGINT,
    precision           VARCHAR(30),
    quantization        VARCHAR(50),
    metadata            JSONB NOT NULL DEFAULT '{}'::jsonb,
    source_url          TEXT,
    source_name         VARCHAR(100),
    source_updated_at   TIMESTAMPTZ,
    metadata_version    INTEGER NOT NULL DEFAULT 1,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT chk_model_metadata_parameter_count
        CHECK (parameter_count IS NULL OR parameter_count > 0),
    CONSTRAINT chk_model_metadata_json_object
        CHECK (jsonb_typeof(metadata) = 'object'),
    CONSTRAINT chk_model_metadata_version
        CHECK (metadata_version > 0)
);

CREATE INDEX IF NOT EXISTS idx_model_metadata_architecture
    ON model_metadata(architecture)
    WHERE architecture IS NOT NULL;

CREATE TABLE IF NOT EXISTS provider_policies (
    id                          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_id                UUID NOT NULL
        REFERENCES model_providers(id) ON DELETE CASCADE,
    model_catalog_id            UUID
        REFERENCES model_catalog(id) ON DELETE CASCADE,
    policy_scope                VARCHAR(20) NOT NULL DEFAULT 'provider',
    retention_mode              VARCHAR(50) NOT NULL,
    retention_days              INTEGER,
    training_usage              VARCHAR(50) NOT NULL,
    provider_access             VARCHAR(50) NOT NULL,
    supported_retention_modes   JSONB NOT NULL DEFAULT '[]'::jsonb,
    data_residency              JSONB NOT NULL DEFAULT '{}'::jsonb,
    security_controls           JSONB NOT NULL DEFAULT '{}'::jsonb,
    compliance                  JSONB NOT NULL DEFAULT '[]'::jsonb,
    limitations                 JSONB NOT NULL DEFAULT '[]'::jsonb,
    privacy_policy_url          TEXT,
    terms_url                   TEXT,
    documentation_url           TEXT,
    source_url                  TEXT,
    policy_version              VARCHAR(50) NOT NULL,
    effective_from              TIMESTAMPTZ NOT NULL,
    effective_to                TIMESTAMPTZ,
    is_current                  BOOLEAN NOT NULL DEFAULT true,
    created_by_admin_id         UUID,
    created_at                  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT chk_provider_policy_scope
        CHECK (policy_scope IN ('provider', 'model')),
    CONSTRAINT chk_provider_policy_scope_target
        CHECK ((policy_scope = 'provider' AND model_catalog_id IS NULL)
            OR (policy_scope = 'model' AND model_catalog_id IS NOT NULL)),
    CONSTRAINT chk_provider_policy_retention_days
        CHECK (retention_days IS NULL OR retention_days >= 0),
    CONSTRAINT chk_provider_policy_modes_array
        CHECK (jsonb_typeof(supported_retention_modes) = 'array'),
    CONSTRAINT chk_provider_policy_residency_object
        CHECK (jsonb_typeof(data_residency) = 'object'),
    CONSTRAINT chk_provider_policy_security_object
        CHECK (jsonb_typeof(security_controls) = 'object'),
    CONSTRAINT chk_provider_policy_compliance_array
        CHECK (jsonb_typeof(compliance) = 'array'),
    CONSTRAINT chk_provider_policy_limitations_array
        CHECK (jsonb_typeof(limitations) = 'array'),
    CONSTRAINT chk_provider_policy_period
        CHECK (effective_to IS NULL OR effective_to > effective_from)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_provider_policies_current_provider
    ON provider_policies(provider_id)
    WHERE is_current = true AND model_catalog_id IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_provider_policies_current_model
    ON provider_policies(provider_id, model_catalog_id)
    WHERE is_current = true AND model_catalog_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_provider_policies_model_history
    ON provider_policies(provider_id, model_catalog_id, effective_from DESC);

CREATE OR REPLACE FUNCTION touch_model_metadata_updated_at()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_model_metadata_updated_at ON model_metadata;
CREATE TRIGGER trg_model_metadata_updated_at
BEFORE UPDATE ON model_metadata
FOR EACH ROW
EXECUTE FUNCTION touch_model_metadata_updated_at();
