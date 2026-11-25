-- =============================================================================
-- ABIFR — Migration 0003: Session Revocation, Password Rotation & Materialization
-- =============================================================================

ALTER TABLE users
    ADD COLUMN IF NOT EXISTS must_rotate_password BOOLEAN NOT NULL DEFAULT FALSE;

CREATE TABLE IF NOT EXISTS user_sessions (
    jti UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    csrf_token VARCHAR(128) NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_user_sessions_user ON user_sessions(user_id, revoked_at);
CREATE INDEX IF NOT EXISTS idx_user_sessions_exp ON user_sessions(expires_at);

ALTER TABLE documents
    ADD COLUMN IF NOT EXISTS materialized_entity_type VARCHAR(32),
    ADD COLUMN IF NOT EXISTS materialized_entity_id UUID,
    ADD COLUMN IF NOT EXISTS materialized_at TIMESTAMPTZ;
