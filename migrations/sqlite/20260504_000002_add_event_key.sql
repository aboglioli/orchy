-- Add event key column for partitioning
ALTER TABLE events ADD COLUMN key TEXT NOT NULL DEFAULT '';
CREATE INDEX IF NOT EXISTS idx_events_key ON events (key);
