-- Product plans. Prices are kept in features until the billing bounded context
-- owns provider-specific price IDs and currency mapping.
INSERT INTO plans (
    name, slug, requests_per_minute, tokens_per_minute,
    max_concurrent_requests, monthly_token_quota, price_multiplier,
    billing_mode, features, is_active,
    storage_bytes_quota, max_file_size_bytes, max_files,
    embeddings_enabled, rag_enabled, qdrant_mode
)
VALUES
(
    'Starter', 'starter', 20, 10000, 2, 100000, 1.4, 'prepaid',
    '{"price_usd":5,"trial_days":3,"max_api_keys":1,"models":"basic","metrics":"basic","fair_use":true}', true,
    10737418240, 10485760, 100, true, true, 'shared'
),
(
    'Pro', 'pro', 120, 200000, 10, 10000000, 1.4, 'postpaid',
    '{"price_usd":10,"max_api_keys":0,"models":"advanced","metrics":"detailed","overage":true,"fair_use":true}', true,
    26843545600, 52428800, 1000, true, true, 'shared'
),
(
    'Pro Plus', 'pro-plus', 300, 1000000, 25, NULL, 1.4, 'postpaid',
    '{"price_usd":20,"enterprise_price_usd":75,"max_api_keys":0,"models":"all","metrics":"detailed","overage":true,"s3_api":true,"dedicated_qdrant":true,"fair_use":true}', true,
    107374182400, 268435456, 10000, true, true, 'shared'
)
ON CONFLICT (slug) DO UPDATE SET
    name = EXCLUDED.name,
    requests_per_minute = EXCLUDED.requests_per_minute,
    tokens_per_minute = EXCLUDED.tokens_per_minute,
    max_concurrent_requests = EXCLUDED.max_concurrent_requests,
    monthly_token_quota = EXCLUDED.monthly_token_quota,
    billing_mode = EXCLUDED.billing_mode,
    features = EXCLUDED.features,
    is_active = EXCLUDED.is_active,
    storage_bytes_quota = EXCLUDED.storage_bytes_quota,
    max_file_size_bytes = EXCLUDED.max_file_size_bytes,
    max_files = EXCLUDED.max_files,
    embeddings_enabled = EXCLUDED.embeddings_enabled,
    rag_enabled = EXCLUDED.rag_enabled,
    qdrant_mode = EXCLUDED.qdrant_mode;
