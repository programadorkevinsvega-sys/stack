-- Database: catalog_db (public)

CREATE TABLE IF NOT EXISTS model_providers (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name       VARCHAR(50) NOT NULL UNIQUE,
    is_active  BOOLEAN NOT NULL DEFAULT true
);

CREATE TABLE IF NOT EXISTS model_catalog (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_id         UUID NOT NULL REFERENCES model_providers(id),
    public_model_name   VARCHAR(100) NOT NULL UNIQUE,
    provider_model_id   VARCHAR(150) NOT NULL,
    family              VARCHAR(50),
    context_window      INT NOT NULL,
    max_output_tokens   INT NOT NULL,
    supports_streaming  BOOLEAN NOT NULL DEFAULT true,
    supports_tools      BOOLEAN NOT NULL DEFAULT false,
    supports_vision     BOOLEAN NOT NULL DEFAULT false,
    is_active           BOOLEAN NOT NULL DEFAULT true,
    deprecated_at       TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT chk_model_context_window_positive CHECK (context_window > 0),
    CONSTRAINT chk_model_max_output_tokens_positive CHECK (max_output_tokens > 0)
);

CREATE TABLE IF NOT EXISTS model_pricing (
    id                       UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    model_catalog_id         UUID NOT NULL REFERENCES model_catalog(id),
    aws_cost_per_1k_input    NUMERIC(12,6) NOT NULL,
    aws_cost_per_1k_output   NUMERIC(12,6) NOT NULL,
    price_per_1k_input       NUMERIC(12,6) NOT NULL,
    price_per_1k_output      NUMERIC(12,6) NOT NULL,
    effective_from           TIMESTAMPTZ NOT NULL DEFAULT now(),
    effective_to             TIMESTAMPTZ,
    created_by_admin_id      UUID,
    CONSTRAINT chk_pricing_positive CHECK (
        aws_cost_per_1k_input >= 0 AND aws_cost_per_1k_output >= 0 AND
        price_per_1k_input >= 0 AND price_per_1k_output >= 0
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_pricing_one_active_per_model
    ON model_pricing(model_catalog_id)
    WHERE effective_to IS NULL;

CREATE INDEX IF NOT EXISTS idx_model_catalog_provider_active
    ON model_catalog(provider_id, is_active, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_model_catalog_family_active
    ON model_catalog(family, is_active, created_at DESC)
    WHERE family IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_model_pricing_model_effective_from
    ON model_pricing(model_catalog_id, effective_from DESC);

-- La relación de visibilidad pertenece al catálogo. plan_id se valida por UUID
-- sin una FK entre bases de datos.
CREATE TABLE IF NOT EXISTS plan_model_access (
    plan_id UUID NOT NULL,
    model_catalog_id UUID NOT NULL REFERENCES model_catalog(id) ON DELETE CASCADE,
    PRIMARY KEY (plan_id, model_catalog_id)
);
