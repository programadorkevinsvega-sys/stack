INSERT INTO model_providers (id, name, is_active)
VALUES
    ('019f0000-0000-7000-8000-000000000001', 'nvidia', true),
    ('019f0000-0000-7000-8000-000000000002', 'mistral-ai', true)
ON CONFLICT (name) DO UPDATE
SET is_active = EXCLUDED.is_active;

INSERT INTO model_catalog (
    id,
    provider_id,
    public_model_name,
    provider_model_id,
    family,
    context_window,
    max_output_tokens,
    supports_streaming,
    supports_tools,
    supports_vision,
    is_active,
    deprecated_at
)
VALUES
(
    '019f0000-0000-7000-8000-000000000101',
    (SELECT id FROM model_providers WHERE name = 'nvidia'),
    'NVIDIA Nemotron Nano 12B v2 VL BF16',
    'nvidia.nemotron-nano-12b-v2',
    'nemotron',
    131072,
    8192,
    false,
    false,
    true,
    true,
    NULL
),
(
    '019f0000-0000-7000-8000-000000000102',
    (SELECT id FROM model_providers WHERE name = 'nvidia'),
    'NVIDIA Nemotron Nano 9B v2',
    'nvidia.nemotron-nano-9b-v2',
    'nemotron',
    131072,
    8192,
    false,
    false,
    false,
    true,
    NULL
),
(
    '019f0000-0000-7000-8000-000000000103',
    (SELECT id FROM model_providers WHERE name = 'mistral-ai'),
    'Ministral 3B',
    'mistral.ministral-3-3b-instruct',
    'ministral',
    131072,
    8192,
    false,
    false,
    true,
    true,
    NULL
)
ON CONFLICT (public_model_name) DO UPDATE SET
    provider_id = EXCLUDED.provider_id,
    provider_model_id = EXCLUDED.provider_model_id,
    family = EXCLUDED.family,
    context_window = EXCLUDED.context_window,
    max_output_tokens = EXCLUDED.max_output_tokens,
    supports_streaming = EXCLUDED.supports_streaming,
    supports_tools = EXCLUDED.supports_tools,
    supports_vision = EXCLUDED.supports_vision,
    is_active = EXCLUDED.is_active,
    deprecated_at = EXCLUDED.deprecated_at;

INSERT INTO model_metadata (
    id,
    model_catalog_id,
    description,
    architecture,
    parameter_count,
    precision,
    quantization,
    metadata,
    source_name,
    metadata_version
)
VALUES
(
    '019f0000-0000-7000-8000-000000000201',
    (SELECT id FROM model_catalog WHERE public_model_name = 'NVIDIA Nemotron Nano 12B v2 VL BF16'),
    'Nemotron Nano 12B v2 VL is NVIDIA''s 12-billion parameter vision-language model for image understanding and visual question answering.',
    'Nemotron Nano v2 VL',
    12000000000,
    'BF16',
    NULL,
    '{
      "modalities": {
        "input": {"text": true, "image": true, "audio": false, "video": false, "speech": false, "embedding": false},
        "output": {"text": true, "image": false, "audio": false, "video": false, "speech": false, "embedding": false}
      },
      "limits": {"context_window_tokens": 131072, "max_output_tokens": 8192},
      "features": [],
      "capabilities": {"multimodal": true, "image_understanding": true, "visual_question_answering": true},
      "pricing_reference": {"region": "us-east-1", "currency": "USD", "input_per_million_tokens": 0.2, "output_per_million_tokens": 0.6}
    }'::jsonb,
    'AWS Bedrock model catalog',
    1
),
(
    '019f0000-0000-7000-8000-000000000202',
    (SELECT id FROM model_catalog WHERE public_model_name = 'NVIDIA Nemotron Nano 9B v2'),
    'Nemotron Nano 9B v2 is NVIDIA''s 9-billion parameter model optimized for efficient text generation, reasoning, and coding on NVIDIA hardware.',
    'Nemotron Nano v2',
    9000000000,
    NULL,
    NULL,
    '{
      "modalities": {
        "input": {"text": true, "image": false, "audio": false, "video": false, "speech": false, "embedding": false},
        "output": {"text": true, "image": false, "audio": false, "video": false, "speech": false, "embedding": false}
      },
      "limits": {"context_window_tokens": 131072, "max_output_tokens": 8192},
      "features": [],
      "capabilities": {"multimodal": false, "reasoning": true, "coding": true},
      "pricing_reference": {"region": "us-east-1", "currency": "USD", "input_per_million_tokens": 0.06, "output_per_million_tokens": 0.23}
    }'::jsonb,
    'AWS Bedrock model catalog',
    1
),
(
    '019f0000-0000-7000-8000-000000000203',
    (SELECT id FROM model_catalog WHERE public_model_name = 'Ministral 3B'),
    'Ministral 3B is Mistral AI''s ultra-compact 3-billion parameter model for on-device tasks requiring minimal compute.',
    'Ministral',
    3000000000,
    NULL,
    NULL,
    '{
      "modalities": {
        "input": {"text": true, "image": true, "audio": false, "video": false, "speech": false, "embedding": false},
        "output": {"text": true, "image": false, "audio": false, "video": false, "speech": false, "embedding": false}
      },
      "limits": {"context_window_tokens": 131072, "max_output_tokens": 8192},
      "features": [],
      "capabilities": {"multimodal": true},
      "pricing_reference": {"region": "us-east-1", "currency": "USD", "input_per_million_tokens": 0.1, "output_per_million_tokens": 0.1}
    }'::jsonb,
    'AWS Bedrock model catalog',
    1
)
ON CONFLICT (model_catalog_id) DO UPDATE SET
    description = EXCLUDED.description,
    architecture = EXCLUDED.architecture,
    parameter_count = EXCLUDED.parameter_count,
    precision = EXCLUDED.precision,
    quantization = EXCLUDED.quantization,
    metadata = EXCLUDED.metadata,
    source_name = EXCLUDED.source_name,
    metadata_version = EXCLUDED.metadata_version,
    updated_at = now();

UPDATE model_pricing
SET effective_to = now()
WHERE model_catalog_id IN (
    (SELECT id FROM model_catalog WHERE public_model_name = 'NVIDIA Nemotron Nano 12B v2 VL BF16'),
    (SELECT id FROM model_catalog WHERE public_model_name = 'NVIDIA Nemotron Nano 9B v2'),
    (SELECT id FROM model_catalog WHERE public_model_name = 'Ministral 3B')
)
AND effective_to IS NULL;

INSERT INTO model_pricing (
    id,
    model_catalog_id,
    aws_cost_per_1k_input,
    aws_cost_per_1k_output,
    price_per_1k_input,
    price_per_1k_output,
    effective_from,
    effective_to,
    created_by_admin_id
)
VALUES
    ('019f0000-0000-7000-8000-000000000301', (SELECT id FROM model_catalog WHERE public_model_name = 'NVIDIA Nemotron Nano 12B v2 VL BF16'), 0.000200, 0.000600, 0.000200, 0.000600, now(), NULL, NULL),
    ('019f0000-0000-7000-8000-000000000302', (SELECT id FROM model_catalog WHERE public_model_name = 'NVIDIA Nemotron Nano 9B v2'), 0.000060, 0.000230, 0.000060, 0.000230, now(), NULL, NULL),
    ('019f0000-0000-7000-8000-000000000303', (SELECT id FROM model_catalog WHERE public_model_name = 'Ministral 3B'), 0.000100, 0.000100, 0.000100, 0.000100, now(), NULL, NULL)
ON CONFLICT (id) DO UPDATE SET
    aws_cost_per_1k_input = EXCLUDED.aws_cost_per_1k_input,
    aws_cost_per_1k_output = EXCLUDED.aws_cost_per_1k_output,
    price_per_1k_input = EXCLUDED.price_per_1k_input,
    price_per_1k_output = EXCLUDED.price_per_1k_output,
    effective_to = EXCLUDED.effective_to;
