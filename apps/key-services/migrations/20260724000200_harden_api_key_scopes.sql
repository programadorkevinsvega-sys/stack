-- Fase 6: scopes are an authorization boundary, not free-form metadata.
-- Keep this additive and validate existing rows before making it strict.
ALTER TABLE api_keys
    ADD CONSTRAINT api_keys_scopes_not_empty
    CHECK (cardinality(scopes) > 0) NOT VALID;

ALTER TABLE api_keys
    ADD CONSTRAINT api_keys_scopes_allowed
    CHECK (scopes <@ ARRAY[
        'inference:read', 'inference:write', 'keys:manage', 'billing:read',
        'files:read', 'files:write', 'files:delete'
    ]::text[]) NOT VALID;

-- These validations fail fast during deployment if an operator inserted an
-- unsupported scope manually; the service never silently broadens access.
ALTER TABLE api_keys VALIDATE CONSTRAINT api_keys_scopes_not_empty;
ALTER TABLE api_keys VALIDATE CONSTRAINT api_keys_scopes_allowed;

CREATE INDEX IF NOT EXISTS idx_api_keys_user_status_created
    ON api_keys(user_id, status, created_at DESC);
