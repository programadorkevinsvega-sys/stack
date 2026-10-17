-- Partial unique index: solo una suscripción activa/trial por usuario (H24+H25)
CREATE UNIQUE INDEX IF NOT EXISTS uq_one_active_subscription_per_user
    ON subscriptions (user_id)
    WHERE status IN ('trialing', 'active');
