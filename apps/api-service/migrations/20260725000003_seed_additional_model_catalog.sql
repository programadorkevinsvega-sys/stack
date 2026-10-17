INSERT INTO model_providers (id, name, is_active)
VALUES
    ('019f0000-0000-7000-8000-000000000003', 'google', true),
    ('019f0000-0000-7000-8000-000000000004', 'openai', true)
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
    '019f0000-0000-7000-8000-000000000104',
    (SELECT id FROM model_providers WHERE name = 'mistral-ai'),
    'Ministral 3 8B',
    'mistral.ministral-3-8b-instruct',
    'ministral',
    131072,
    8192,
    false,
    false,
    true,
    true,
    NULL
),
(
    '019f0000-0000-7000-8000-000000000105',
    (SELECT id FROM model_providers WHERE name = 'mistral-ai'),
    'Ministral 14B 3.0',
    'mistral.ministral-3-14b-instruct',
    'ministral',
    131072,
    8192,
    false,
    false,
    true,
    true,
    NULL
),
(
    '019f0000-0000-7000-8000-000000000106',
    (SELECT id FROM model_providers WHERE name = 'google'),
    'Gemma 3 4B IT',
    'google.gemma-3-4b-it',
    'gemma',
    131072,
    8192,
    false,
    false,
    true,
    true,
    NULL
),
(
    '019f0000-0000-7000-8000-000000000107',
    (SELECT id FROM model_providers WHERE name = 'openai'),
    'GPT OSS Safeguard 20B',
    'openai.gpt-oss-safeguard-20b',
    'gpt-oss-safeguard',
    131072,
    16384,
    false,
    false,
    false,
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
    '019f0000-0000-7000-8000-000000000204',
    (SELECT id FROM model_catalog WHERE public_model_name = 'Ministral 3 8B'),
    'Ministral 3 8B is Mistral AI''s 8-billion parameter compact model for edge and mobile deployment with efficient inference.',
    'Ministral 3',
    8000000000,
    NULL,
    NULL,
    '{
      "modalities": {
        "input": {"text": true, "image": true, "audio": false, "video": false, "speech": false, "embedding": false},
        "output": {"text": true, "image": false, "audio": false, "video": false, "speech": false, "embedding": false}
      },
      "limits": {"context_window_tokens": 131072, "max_output_tokens": 8192},
      "features": [],
      "capabilities": {"multimodal": true, "edge_deployment": true, "mobile_deployment": true},
      "pricing_reference": {"region": "us-east-1", "currency": "USD", "input_per_million_tokens": 0.15, "output_per_million_tokens": 0.15}
    }'::jsonb,
    'AWS Bedrock model catalog',
    1
),
(
    '019f0000-0000-7000-8000-000000000205',
    (SELECT id FROM model_catalog WHERE public_model_name = 'Ministral 14B 3.0'),
    'Ministral 14B 3.0 is Mistral AI''s 14-billion parameter edge model optimized for on-device deployment with strong knowledge and reasoning performance.',
    'Ministral 3',
    14000000000,
    NULL,
    NULL,
    '{
      "modalities": {
        "input": {"text": true, "image": true, "audio": false, "video": false, "speech": false, "embedding": false},
        "output": {"text": true, "image": false, "audio": false, "video": false, "speech": false, "embedding": false}
      },
      "limits": {"context_window_tokens": 131072, "max_output_tokens": 8192},
      "features": [],
      "capabilities": {"multimodal": true, "edge_deployment": true, "on_device_deployment": true, "reasoning": true},
      "pricing_reference": {"region": "us-east-1", "currency": "USD", "input_per_million_tokens": 0.2, "output_per_million_tokens": 0.2}
    }'::jsonb,
    'AWS Bedrock model catalog',
    1
),
(
    '019f0000-0000-7000-8000-000000000206',
    (SELECT id FROM model_catalog WHERE public_model_name = 'Gemma 3 4B IT'),
    'Gemma 3 4B IT is Google''s compact 4-billion parameter open model with instruction tuning for on-device and edge deployment.',
    'Gemma 3',
    4000000000,
    NULL,
    NULL,
    '{
      "modalities": {
        "input": {"text": true, "image": true, "audio": false, "video": false, "speech": false, "embedding": false},
        "output": {"text": true, "image": false, "audio": false, "video": false, "speech": false, "embedding": false}
      },
      "limits": {"context_window_tokens": 131072, "max_output_tokens": 8192},
      "features": [],
      "capabilities": {"multimodal": true, "instruction_tuned": true, "edge_deployment": true},
      "pricing_reference": {"region": "us-east-1", "currency": "USD", "input_per_million_tokens": 0.04, "output_per_million_tokens": 0.08}
    }'::jsonb,
    'AWS Bedrock model catalog',
    1
),
(
    '019f0000-0000-7000-8000-000000000207',
    (SELECT id FROM model_catalog WHERE public_model_name = 'GPT OSS Safeguard 20B'),
    'GPT OSS Safeguard 20B is OpenAI''s compact 20-billion parameter open-source safety model for content moderation and guardrail tasks.',
    'GPT OSS Safeguard',
    20000000000,
    NULL,
    NULL,
    '{
      "modalities": {
        "input": {"text": true, "image": false, "audio": false, "video": false, "speech": false, "embedding": false},
        "output": {"text": true, "image": false, "audio": false, "video": false, "speech": false, "embedding": false}
      },
      "limits": {"context_window_tokens": 131072, "max_output_tokens": 16384},
      "features": [],
      "capabilities": {"multimodal": false, "content_moderation": true, "guardrails": true, "safety": true},
      "pricing_reference": {"region": "us-east-1", "currency": "USD", "input_per_million_tokens": 0.07, "output_per_million_tokens": 0.2}
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
    (SELECT id FROM model_catalog WHERE public_model_name = 'Ministral 3 8B'),
    (SELECT id FROM model_catalog WHERE public_model_name = 'Ministral 14B 3.0'),
    (SELECT id FROM model_catalog WHERE public_model_name = 'Gemma 3 4B IT'),
    (SELECT id FROM model_catalog WHERE public_model_name = 'GPT OSS Safeguard 20B')
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
    ('019f0000-0000-7000-8000-000000000304', (SELECT id FROM model_catalog WHERE public_model_name = 'Ministral 3 8B'), 0.000150, 0.000150, 0.000150, 0.000150, now(), NULL, NULL),
    ('019f0000-0000-7000-8000-000000000305', (SELECT id FROM model_catalog WHERE public_model_name = 'Ministral 14B 3.0'), 0.000200, 0.000200, 0.000200, 0.000200, now(), NULL, NULL),
    ('019f0000-0000-7000-8000-000000000306', (SELECT id FROM model_catalog WHERE public_model_name = 'Gemma 3 4B IT'), 0.000040, 0.000080, 0.000040, 0.000080, now(), NULL, NULL),
    ('019f0000-0000-7000-8000-000000000307', (SELECT id FROM model_catalog WHERE public_model_name = 'GPT OSS Safeguard 20B'), 0.000070, 0.000200, 0.000070, 0.000200, now(), NULL, NULL)
ON CONFLICT (id) DO UPDATE SET
    aws_cost_per_1k_input = EXCLUDED.aws_cost_per_1k_input,
    aws_cost_per_1k_output = EXCLUDED.aws_cost_per_1k_output,
    price_per_1k_input = EXCLUDED.price_per_1k_input,
    price_per_1k_output = EXCLUDED.price_per_1k_output,
    effective_to = EXCLUDED.effective_to;
