ALTER TABLE api_keys RENAME COLUMN key TO key_hash;
ALTER TABLE api_keys ADD COLUMN IF NOT EXISTS key_prefix TEXT NOT NULL DEFAULT '';
UPDATE api_keys SET key_prefix = substr(key_hash, 1, 8) WHERE key_prefix = '';
