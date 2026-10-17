ALTER TABLE folders ADD COLUMN IF NOT EXISTS idempotency_key VARCHAR(255);
CREATE UNIQUE INDEX IF NOT EXISTS uq_folders_user_idempotency ON folders(user_id,idempotency_key) WHERE idempotency_key IS NOT NULL;
