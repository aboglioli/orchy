-- Add seq column to events table for event ordering in reader subscriptions
ALTER TABLE events ADD COLUMN seq INTEGER;

-- Populate seq with rowid for existing events
UPDATE events SET seq = rowid WHERE seq IS NULL;

-- Create unique index on seq
CREATE UNIQUE INDEX IF NOT EXISTS idx_events_seq ON events (seq);

-- Create composite index for org-specific seq reads
CREATE INDEX IF NOT EXISTS idx_events_org_seq ON events (organization, seq);

-- Consumer offsets table for tracking subscription progress per consumer group
CREATE TABLE IF NOT EXISTS consumer_offsets (
    group_id TEXT PRIMARY KEY,
    last_seq INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL
);
