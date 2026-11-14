-- OIDC (Keycloak) login: users are linked to the IdP `sub` manually via SQL.
ALTER TABLE users ADD COLUMN IF NOT EXISTS external_id TEXT UNIQUE;
-- Kept for rollback; dropped in a later migration.
ALTER TABLE users ALTER COLUMN password_hash DROP NOT NULL;

-- Server-side sessions. The cookie holds the raw id; only its SHA-256 hex is stored.
CREATE TABLE IF NOT EXISTS sessions (
  id_hash TEXT PRIMARY KEY,
  user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  csrf_token TEXT NOT NULL,
  id_token TEXT,
  user_agent TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS sessions_expires_at_idx ON sessions (expires_at);
CREATE INDEX IF NOT EXISTS sessions_user_id_idx ON sessions (user_id);
