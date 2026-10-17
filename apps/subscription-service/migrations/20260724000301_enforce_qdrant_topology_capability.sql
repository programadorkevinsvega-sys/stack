-- Fase 5A: collection topology is an explicit contractual capability.
-- A plan slug or price tier must never implicitly grant a dedicated collection.
UPDATE plans
SET qdrant_mode = 'shared',
    features = (features - 'dedicated_qdrant') ||
               jsonb_build_object(
                   'dedicated_qdrant_enabled',
                   CASE
                       WHEN slug = 'pro-plus'
                           THEN false
                       ELSE COALESCE((features->>'dedicated_qdrant_enabled')::boolean, false)
                   END
               )
WHERE qdrant_mode = 'dedicated'
   OR features ? 'dedicated_qdrant';

-- Normalize any legacy dedicated mode without an explicit contractual flag.
UPDATE plans
SET qdrant_mode = 'shared'
WHERE qdrant_mode = 'dedicated'
  AND COALESCE((features->>'dedicated_qdrant_enabled')::boolean, false) = false;

ALTER TABLE plans
    ADD CONSTRAINT plans_dedicated_qdrant_requires_shared_mode
    CHECK (qdrant_mode <> 'dedicated' OR
           COALESCE((features->>'dedicated_qdrant_enabled')::boolean, false));

CREATE INDEX IF NOT EXISTS idx_plans_qdrant_topology
    ON plans (qdrant_mode, is_active)
    WHERE qdrant_mode IN ('shared', 'dedicated');
