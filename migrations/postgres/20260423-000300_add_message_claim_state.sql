-- Add claim state to messages for logical target coordination
ALTER TABLE messages ADD COLUMN IF NOT EXISTS claimed_by TEXT;
ALTER TABLE messages ADD COLUMN IF NOT EXISTS claimed_at TIMESTAMPTZ;
