-- Sync database schema to match current domain models
-- Fix agents table: add alias, rename last_heartbeat to last_seen
-- Fix tasks table: add stale_after_secs, last_activity_at
-- Fix knowledge_entries table: add valid_from, valid_until

-- Agents: add alias column and rename last_heartbeat to last_seen
-- SQLite doesn't support DROP COLUMN or RENAME COLUMN directly, so recreate
ALTER TABLE agents RENAME TO agents_old;

CREATE TABLE agents (
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

INSERT INTO agents (id, alias, organization_id, project, namespace, roles, description, status, last_seen, connected_at, metadata)
SELECT id, '', organization_id, project, namespace, roles, description, status, last_heartbeat, connected_at, metadata FROM agents_old;

DROP TABLE agents_old;
CREATE UNIQUE INDEX idx_agents_alias_unique ON agents (organization_id, project, alias);
CREATE INDEX idx_agents_status_heartbeat ON agents (status, last_seen);

-- Tasks: add stale_after_secs and last_activity_at
-- last_activity_at defaults to epoch, updated to created_at for existing rows
ALTER TABLE tasks ADD COLUMN stale_after_secs INTEGER;
ALTER TABLE tasks ADD COLUMN last_activity_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00+00:00';

-- Knowledge entries: add validity columns
ALTER TABLE knowledge_entries ADD COLUMN valid_from TEXT;
ALTER TABLE knowledge_entries ADD COLUMN valid_until TEXT;
