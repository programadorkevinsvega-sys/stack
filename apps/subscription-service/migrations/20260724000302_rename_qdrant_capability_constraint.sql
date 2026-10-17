ALTER TABLE plans
    RENAME CONSTRAINT plans_dedicated_qdrant_requires_shared_mode
    TO plans_dedicated_qdrant_requires_capability;
