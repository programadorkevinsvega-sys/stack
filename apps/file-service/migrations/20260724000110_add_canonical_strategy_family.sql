-- Canonical Qdrant points carry the embedding strategy family explicitly.
ALTER TABLE embedding_vectors
    ADD COLUMN IF NOT EXISTS strategy_family VARCHAR(64) NOT NULL DEFAULT 'legacy-v1';

CREATE INDEX IF NOT EXISTS idx_embedding_vectors_strategy_family
    ON embedding_vectors(user_id, strategy_family, embedding_model, embedding_version);
