ALTER TABLE api_keys ADD COLUMN IF NOT EXISTS key_suffix TEXT NOT NULL DEFAULT '';
UPDATE api_keys SET key_suffix = substr(key_hash, length(key_hash) - 3, 4) WHERE key_suffix = '';
