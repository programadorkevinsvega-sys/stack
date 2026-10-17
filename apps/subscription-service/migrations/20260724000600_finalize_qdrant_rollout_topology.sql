-- Fase 9A: repair databases that were migrated before dedicated Qdrant became
-- an explicit contractual capability.  This is intentionally conservative:
-- existing accounts remain shared until an administrator explicitly grants
-- dedicated_qdrant_enabled=true.
UPDATE plans
SET qdrant_mode = CASE WHEN qdrant_mode = 'none' THEN 'none' ELSE 'shared' END,
    features = (features - 'dedicated_qdrant') ||
               jsonb_build_object('dedicated_qdrant_enabled', false)
WHERE qdrant_mode IN ('shared','dedicated')
   OR features ? 'dedicated_qdrant'
   OR features ? 'dedicated_qdrant_enabled';

ALTER TABLE plans DROP CONSTRAINT IF EXISTS plans_dedicated_qdrant_requires_capability;
ALTER TABLE plans
    ADD CONSTRAINT plans_dedicated_qdrant_requires_capability
    CHECK (qdrant_mode <> 'dedicated'
           OR COALESCE((features->>'dedicated_qdrant_enabled')::boolean, false));

CREATE INDEX IF NOT EXISTS idx_plans_qdrant_rollout
    ON plans (qdrant_mode, is_active)
    WHERE qdrant_mode IN ('shared','dedicated');
