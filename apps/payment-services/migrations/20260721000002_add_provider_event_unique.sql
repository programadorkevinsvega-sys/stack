-- Agrega constraint UNIQUE para evitar webhooks duplicados (C4).
-- PostgreSQL no soporta ADD CONSTRAINT IF NOT EXISTS.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'uq_provider_event'
          AND conrelid = 'payment_provider_events'::regclass
    ) THEN
        ALTER TABLE payment_provider_events
            ADD CONSTRAINT uq_provider_event UNIQUE (provider, provider_event_id);
    END IF;
END
$$;
