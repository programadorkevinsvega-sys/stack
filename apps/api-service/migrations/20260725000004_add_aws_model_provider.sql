ALTER TABLE model_providers
    ADD COLUMN IF NOT EXISTS contact_email VARCHAR(320);

ALTER TABLE model_providers
    DROP CONSTRAINT IF EXISTS chk_model_provider_contact_email;

ALTER TABLE model_providers
    ADD CONSTRAINT chk_model_provider_contact_email
    CHECK (
        contact_email IS NULL
        OR contact_email ~ '^[^[:space:]@]+@[^[:space:]@]+\.[^[:space:]@]+$'
    );

ALTER TABLE model_metadata
    ADD COLUMN IF NOT EXISTS serving_provider_id UUID
        REFERENCES model_providers(id) ON DELETE RESTRICT;

INSERT INTO model_providers (id, name, contact_email, is_active)
VALUES (
    '019f0000-0000-7000-8000-000000000005',
    'aws',
    'medellinpublico12@gmail.com',
    true
)
ON CONFLICT (name) DO UPDATE SET
    contact_email = EXCLUDED.contact_email,
    is_active = EXCLUDED.is_active;

UPDATE model_metadata
SET serving_provider_id = (
    SELECT id FROM model_providers WHERE name = 'aws'
)
WHERE model_catalog_id IN (
    SELECT id
    FROM model_catalog
    WHERE provider_model_id IN (
        'nvidia.nemotron-nano-12b-v2',
        'nvidia.nemotron-nano-9b-v2',
        'mistral.ministral-3-3b-instruct',
        'mistral.ministral-3-8b-instruct',
        'mistral.ministral-3-14b-instruct',
        'google.gemma-3-4b-it',
        'openai.gpt-oss-safeguard-20b'
    )
);

CREATE INDEX IF NOT EXISTS idx_model_metadata_serving_provider
    ON model_metadata(serving_provider_id)
    WHERE serving_provider_id IS NOT NULL;
