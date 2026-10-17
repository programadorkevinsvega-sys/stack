-- Agrega constraint UNIQUE para evitar registros duplicados de cuentas AWS (C11).
-- PostgreSQL no soporta ADD CONSTRAINT IF NOT EXISTS.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'uq_aws_account_id'
          AND conrelid = 'aws_credential_pool'::regclass
    ) THEN
        ALTER TABLE aws_credential_pool
            ADD CONSTRAINT uq_aws_account_id UNIQUE (aws_account_id);
    END IF;
END
$$;
