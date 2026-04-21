-- Fix agents table: rename last_heartbeat to last_seen
-- Note: Postgres migration 20260420-000800_remove_unused_columns.sql correctly did NOT remove these columns
-- But the column is named last_heartbeat while code expects last_seen

ALTER TABLE agents RENAME COLUMN last_heartbeat TO last_seen;

-- Verify tasks table has required columns (should already exist)
-- These columns were added in the initial schema and should be present
-- ALTER TABLE tasks ADD COLUMN IF NOT EXISTS stale_after_secs INTEGER;
-- ALTER TABLE tasks ADD COLUMN IF NOT EXISTS last_activity_at TIMESTAMPTZ;

-- Recreate indexes for agents (index name references last_seen now)
DROP INDEX IF EXISTS idx_agents_status_heartbeat;
CREATE INDEX IF NOT EXISTS idx_agents_status_last_seen ON agents (status, last_seen);
