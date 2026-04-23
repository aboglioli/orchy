-- Add user_id to agents for auth-derived ownership
ALTER TABLE agents ADD COLUMN IF NOT EXISTS user_id TEXT;
