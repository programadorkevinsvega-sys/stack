ALTER TABLE folders ADD COLUMN IF NOT EXISTS status VARCHAR(20) NOT NULL DEFAULT 'active';
ALTER TABLE folders DROP CONSTRAINT IF EXISTS folders_status_check;
ALTER TABLE folders ADD CONSTRAINT folders_status_check CHECK (status IN ('active','deleted'));
CREATE INDEX IF NOT EXISTS idx_folders_user_active ON folders(user_id,parent_id) WHERE status='active';
