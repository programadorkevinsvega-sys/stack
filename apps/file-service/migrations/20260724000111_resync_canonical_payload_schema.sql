-- Force the maintenance worker to rewrite existing points with the complete
-- Fase 5A payload, including strategy_family and the v2 schema.
UPDATE embedding_vectors
SET qdrant_synced = false,
    updated_at = now()
WHERE status = 'ready';
