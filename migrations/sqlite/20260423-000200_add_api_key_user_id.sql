-- Add user_id to api_keys for key ownership
ALTER TABLE api_keys ADD COLUMN user_id TEXT;
