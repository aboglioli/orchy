-- Add user_id to agents for auth-derived ownership
ALTER TABLE agents ADD COLUMN user_id TEXT;
