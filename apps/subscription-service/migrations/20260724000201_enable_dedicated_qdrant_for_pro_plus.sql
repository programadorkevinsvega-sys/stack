-- Pro Plus is the highest product tier in the current catalog. It receives
-- the dedicated-per-user Qdrant mode that was originally described as
-- Enterprise, while Starter and Pro remain in the shared environment
-- collection with mandatory user_id filters.
UPDATE plans
SET qdrant_mode = 'dedicated',
    features = jsonb_set(features, '{dedicated_qdrant}', 'true'::jsonb, true)
WHERE slug = 'pro-plus';
