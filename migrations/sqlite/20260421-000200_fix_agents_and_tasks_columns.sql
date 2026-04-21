-- Fix agents table: add alias and rename last_heartbeat to last_seen
-- The migration 20260420-000800_remove_unused_columns.sql incorrectly removed these columns

-- Rename last_heartbeat to last_seen
-- SQLite doesn't support ALTER TABLE RENAME COLUMN directly, so we recreate
CREATE TABLE agents_backup (
    id TEXT PRIMARY KEY,
    alias TEXT NOT NULL DEFAULT '',
    organization_id TEXT NOT NULL DEFAULT 'default',
    project TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT '/',
    roles TEXT NOT NULL DEFAULT '[]',
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'online',
    last_seen TEXT NOT NULL,
    connected_at TEXT NOT NULL,
    metadata TEXT NOT NULL DEFAULT '{}'
);
INSERT INTO agents_backup SELECT
    id, '', organization_id, project, namespace, roles, description, status, last_heartbeat, connected_at, metadata
FROM agents;
DROP TABLE agents;
ALTER TABLE agents_backup RENAME TO agents;

-- Fix tasks table: add stale_after_secs and last_activity_at columns
-- The migration 20260420-000800_remove_unused_columns.sql incorrectly removed these columns
ALTER TABLE tasks ADD COLUMN stale_after_secs INTEGER;
ALTER TABLE tasks ADD COLUMN last_activity_at TEXT;

-- Update last_activity_at from updated_at for existing tasks
UPDATE tasks SET last_activity_at = updated_at WHERE last_activity_at IS NULL;

-- Add acceptance_criteria if missing (may have been lost during table recreation)
-- Note: this column should already exist from earlier migrations, but adding defensively
-- ALTER TABLE tasks ADD COLUMN acceptance_criteria TEXT;

-- Recreate indexes for agents
CREATE INDEX IF NOT EXISTS idx_agents_status_heartbeat ON agents (status, last_seen);
