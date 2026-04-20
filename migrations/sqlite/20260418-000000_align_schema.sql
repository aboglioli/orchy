-- Align old DB schema with current codebase (post-refactor)
-- Note: refs column is only on messages (tasks and knowledge don't use refs in domain model)

-- 1. Add refs column to messages (ResourceRef support) - skip if already exists
-- SQLite doesn't support IF NOT EXISTS on ALTER TABLE, so we catch errors
-- This is only needed for old databases; new ones have it in initial schema

-- 3. Create message_receipts table (broadcast + role receipt model)
CREATE TABLE IF NOT EXISTS message_receipts (
    message_id TEXT NOT NULL REFERENCES messages(id),
    agent_id TEXT NOT NULL,
    read_at TEXT NOT NULL,
    PRIMARY KEY (message_id, agent_id)
);

-- 4. Create organizations table if missing
CREATE TABLE IF NOT EXISTS organizations (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- 5. Create api_keys table if missing
CREATE TABLE IF NOT EXISTS api_keys (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    name TEXT NOT NULL DEFAULT '',
    key TEXT NOT NULL UNIQUE,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS api_keys_organization_idx ON api_keys (organization_id);
