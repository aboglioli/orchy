-- Event key column is created by the initial schema; this migration only adds the index.
CREATE INDEX IF NOT EXISTS idx_events_key ON events (key);
