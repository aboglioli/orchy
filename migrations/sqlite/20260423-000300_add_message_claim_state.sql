-- Add claim state to messages for logical target coordination
ALTER TABLE messages ADD COLUMN claimed_by TEXT;
ALTER TABLE messages ADD COLUMN claimed_at TEXT;
