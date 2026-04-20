-- Manual migration for orchy.db from pre-edge schema to current schema
-- This handles the edge table creation and legacy relation migration

-- ============================================================================
-- STEP 1: Create edges table with all required columns
-- ============================================================================

CREATE TABLE IF NOT EXISTS edges (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL,
    from_kind TEXT NOT NULL,
    from_id TEXT NOT NULL,
    to_kind TEXT NOT NULL,
    to_id TEXT NOT NULL,
    rel_type TEXT NOT NULL,
    display TEXT,
    created_at TEXT NOT NULL,
    created_by TEXT,
    source_kind TEXT,
    source_id TEXT,
    valid_until TEXT
);

-- ============================================================================
-- STEP 2: Create indexes on edges
-- ============================================================================

CREATE INDEX IF NOT EXISTS idx_edges_from ON edges (org_id, from_kind, from_id);
CREATE INDEX IF NOT EXISTS idx_edges_to ON edges (org_id, to_kind, to_id);
CREATE INDEX IF NOT EXISTS idx_edges_rel_type ON edges (org_id, rel_type);
CREATE INDEX IF NOT EXISTS idx_edges_valid_until ON edges (org_id, valid_until);

-- Edge uniqueness and covering indexes (from 20260420-000700)
CREATE UNIQUE INDEX IF NOT EXISTS idx_edges_unique_active
    ON edges (org_id, from_kind, from_id, to_kind, to_id, rel_type)
    WHERE valid_until IS NULL;

CREATE INDEX IF NOT EXISTS idx_edges_outgoing
    ON edges (org_id, from_kind, from_id, rel_type, to_kind, to_id, created_at)
    WHERE valid_until IS NULL;

CREATE INDEX IF NOT EXISTS idx_edges_incoming
    ON edges (org_id, to_kind, to_id, rel_type, from_kind, from_id, created_at)
    WHERE valid_until IS NULL;

-- ============================================================================
-- STEP 3: Migrate existing refs to edges (knowledge refs from 20260419-000000)
-- ============================================================================

INSERT OR IGNORE INTO edges (id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at)
SELECT
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-7' ||
    substr(lower(hex(randomblob(2))), 2) || '-' || lower(hex(randomblob(2))) || '-' ||
    lower(hex(randomblob(6))) AS id,
    ke.organization_id AS org_id,
    'knowledge' AS from_kind,
    ke.id AS from_id,
    json_extract(r.value, '$.kind') AS to_kind,
    json_extract(r.value, '$.id') AS to_id,
    'related_to' AS rel_type,
    json_extract(r.value, '$.display') AS display,
    ke.updated_at AS created_at
FROM knowledge_entries ke,
     json_each(COALESCE(ke.refs, '[]')) AS r
WHERE json_extract(r.value, '$.id') IS NOT NULL;

-- ============================================================================
-- STEP 4: Migrate existing refs to edges (task refs from 20260419-000000)
-- ============================================================================

INSERT OR IGNORE INTO edges (id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at)
SELECT
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-7' ||
    substr(lower(hex(randomblob(2))), 2) || '-' || lower(hex(randomblob(2))) || '-' ||
    lower(hex(randomblob(6))) AS id,
    t.organization_id AS org_id,
    'task' AS from_kind,
    t.id AS from_id,
    json_extract(r.value, '$.kind') AS to_kind,
    json_extract(r.value, '$.id') AS to_id,
    'related_to' AS rel_type,
    json_extract(r.value, '$.display') AS display,
    t.updated_at AS created_at
FROM tasks t,
     json_each(COALESCE(t.refs, '[]')) AS r
WHERE json_extract(r.value, '$.id') IS NOT NULL;

-- ============================================================================
-- STEP 5: Migrate legacy relation fields to edges (from 20260420-000500)
-- ============================================================================

-- Task parent-child → Spawns edges (parent spawned child)
INSERT OR IGNORE INTO edges (id, org_id, from_kind, from_id, to_kind, to_id, rel_type, created_at)
SELECT
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-7' ||
    substr(lower(hex(randomblob(2))), 2) || '-' || lower(hex(randomblob(2))) || '-' ||
    lower(hex(randomblob(6))),
    organization_id,
    'task',
    CAST(parent_id AS TEXT),
    'task',
    CAST(id AS TEXT),
    'spawns',
    created_at
FROM tasks
WHERE parent_id IS NOT NULL;

-- Task dependencies → DependsOn edges
INSERT OR IGNORE INTO edges (id, org_id, from_kind, from_id, to_kind, to_id, rel_type, created_at)
SELECT
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-7' ||
    substr(lower(hex(randomblob(2))), 2) || '-' || lower(hex(randomblob(2))) || '-' ||
    lower(hex(randomblob(6))),
    t.organization_id,
    'task',
    CAST(t.id AS TEXT),
    'task',
    json_each.value,
    'depends_on',
    t.created_at
FROM tasks t, json_each(t.depends_on)
WHERE t.depends_on IS NOT NULL
  AND t.depends_on != '[]'
  AND t.depends_on != '';

-- Knowledge authorship → OwnedBy edges
INSERT OR IGNORE INTO edges (id, org_id, from_kind, from_id, to_kind, to_id, rel_type, created_at)
SELECT
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-7' ||
    substr(lower(hex(randomblob(2))), 2) || '-' || lower(hex(randomblob(2))) || '-' ||
    lower(hex(randomblob(6))),
    organization_id,
    'knowledge',
    CAST(id AS TEXT),
    'agent',
    CAST(agent_id AS TEXT),
    'owned_by',
    created_at
FROM knowledge_entries
WHERE agent_id IS NOT NULL;

-- Agent parent hierarchy → Spawns edges (parent spawned child agent)
INSERT OR IGNORE INTO edges (id, org_id, from_kind, from_id, to_kind, to_id, rel_type, created_at)
SELECT
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-7' ||
    substr(lower(hex(randomblob(2))), 2) || '-' || lower(hex(randomblob(2))) || '-' ||
    lower(hex(randomblob(6))),
    organization_id,
    'agent',
    CAST(parent_id AS TEXT),
    'agent',
    CAST(id AS TEXT),
    'spawns',
    connected_at
FROM agents
WHERE parent_id IS NOT NULL;

-- ============================================================================
-- STEP 6: Fix edge rel_type references (from 20260419-000200)
-- ============================================================================

UPDATE edges SET rel_type = 'related_to' WHERE rel_type = 'references';

-- ============================================================================
-- STEP 7: Add missing columns to tables (idempotent - ignore if exists)
-- ============================================================================

-- Add acceptance_criteria to tasks (20260419-000100)
-- SQLite doesn't support IF NOT EXISTS for ALTER TABLE, so we catch errors
-- The application should handle this gracefully

-- Add users table (20260420-000100)
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    email TEXT UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT 1,
    is_platform_admin BOOLEAN NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);

CREATE TABLE IF NOT EXISTS org_memberships (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(user_id, org_id)
);

CREATE INDEX IF NOT EXISTS idx_memberships_user ON org_memberships(user_id);
CREATE INDEX IF NOT EXISTS idx_memberships_org ON org_memberships(org_id);

-- ============================================================================
-- STEP 8: Add missing indexes (from 20260420-000000)
-- ============================================================================

CREATE INDEX IF NOT EXISTS idx_agents_status_heartbeat ON agents (status, last_heartbeat);
CREATE INDEX IF NOT EXISTS idx_tasks_assigned_to ON tasks (assigned_to);
CREATE INDEX IF NOT EXISTS idx_tasks_parent_id ON tasks (parent_id);
CREATE INDEX IF NOT EXISTS idx_tasks_status_priority ON tasks (status, priority);
CREATE INDEX IF NOT EXISTS idx_messages_to_target ON messages (to_target);
CREATE INDEX IF NOT EXISTS idx_events_org_timestamp ON events (organization, timestamp);

-- ============================================================================
-- STEP 9: Drop legacy relation columns (from 20260420-000600)
-- ============================================================================

-- Note: SQLite doesn't support DROP COLUMN in older versions.
-- For modern SQLite (3.35.0+), these work. If they fail, the application
-- should ignore these legacy columns.

-- Create new tables without the columns and copy data (SQLite-safe approach)

-- Tasks without parent_id and depends_on
CREATE TABLE tasks_new (
    id TEXT PRIMARY KEY,
    project TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT '/',
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending',
    priority TEXT NOT NULL DEFAULT 'normal',
    assigned_roles TEXT NOT NULL DEFAULT '[]',
    assigned_to TEXT,
    assigned_at TEXT,
    tags TEXT NOT NULL DEFAULT '[]',
    result_summary TEXT,
    notes TEXT NOT NULL DEFAULT '[]',
    created_by TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    organization_id TEXT NOT NULL DEFAULT 'default',
    refs TEXT NOT NULL DEFAULT '[]'
);

INSERT INTO tasks_new SELECT
    id, project, namespace, title, description, status, priority,
    assigned_roles, assigned_to, assigned_at, tags, result_summary,
    notes, created_by, created_at, updated_at, organization_id, refs
FROM tasks;

DROP TABLE tasks;
ALTER TABLE tasks_new RENAME TO tasks;

-- Recreate task indexes
CREATE INDEX IF NOT EXISTS idx_tasks_assigned_to ON tasks (assigned_to);
CREATE INDEX IF NOT EXISTS idx_tasks_status_priority ON tasks (status, priority);

-- Knowledge_entries without agent_id
CREATE TABLE knowledge_entries_new (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL DEFAULT 'default',
    project TEXT,
    namespace TEXT NOT NULL DEFAULT '/',
    path TEXT NOT NULL,
    kind TEXT NOT NULL,
    title TEXT NOT NULL DEFAULT '',
    content TEXT NOT NULL DEFAULT '',
    tags TEXT NOT NULL DEFAULT '[]',
    version INTEGER NOT NULL DEFAULT 1,
    metadata TEXT NOT NULL DEFAULT '{}',
    embedding BLOB,
    embedding_model TEXT,
    embedding_dimensions INTEGER,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    refs TEXT NOT NULL DEFAULT '[]'
);

INSERT INTO knowledge_entries_new SELECT
    id, organization_id, project, namespace, path, kind, title, content,
    tags, version, metadata, embedding, embedding_model, embedding_dimensions,
    created_at, updated_at, refs
FROM knowledge_entries;

DROP TABLE knowledge_entries;
ALTER TABLE knowledge_entries_new RENAME TO knowledge_entries;

-- Recreate knowledge indexes
CREATE UNIQUE INDEX IF NOT EXISTS knowledge_entries_project_path_idx
    ON knowledge_entries (organization_id, project, namespace, path)
    WHERE project IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS knowledge_entries_org_path_idx
    ON knowledge_entries (organization_id, namespace, path)
    WHERE project IS NULL;
CREATE INDEX IF NOT EXISTS knowledge_entries_type_idx ON knowledge_entries (kind);

-- Agents without parent_id
CREATE TABLE agents_new (
    id TEXT PRIMARY KEY,
    project TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT '/',
    roles TEXT NOT NULL DEFAULT '[]',
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'online',
    last_heartbeat TEXT NOT NULL,
    connected_at TEXT NOT NULL,
    metadata TEXT NOT NULL DEFAULT '{}',
    alias TEXT,
    organization_id TEXT NOT NULL DEFAULT 'default'
);

INSERT INTO agents_new SELECT
    id, project, namespace, roles, description, status, last_heartbeat,
    connected_at, metadata, alias, organization_id
FROM agents;

DROP TABLE agents;
ALTER TABLE agents_new RENAME TO agents;

-- Recreate agent indexes
CREATE UNIQUE INDEX IF NOT EXISTS agents_project_alias_idx ON agents (organization_id, project, alias) WHERE alias IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_agents_status_heartbeat ON agents (status, last_heartbeat);

-- ============================================================================
-- STEP 10: Recreate FTS triggers (they were dropped with tables)
-- ============================================================================

CREATE TRIGGER IF NOT EXISTS trg_knowledge_entries_ai AFTER INSERT ON knowledge_entries BEGIN
    INSERT INTO knowledge_entries_fts(knowledge_id, path, title, content)
    VALUES (new.id, new.path, new.title, new.content);
END;

CREATE TRIGGER IF NOT EXISTS trg_knowledge_entries_ad AFTER DELETE ON knowledge_entries BEGIN
    DELETE FROM knowledge_entries_fts WHERE knowledge_id = old.id;
END;

CREATE TRIGGER IF NOT EXISTS trg_knowledge_entries_au AFTER UPDATE ON knowledge_entries BEGIN
    UPDATE knowledge_entries_fts
    SET path = new.path, title = new.title, content = new.content
    WHERE knowledge_id = old.id;
END;

-- ============================================================================
-- STEP 11: Record all migrations as applied
-- ============================================================================

INSERT OR REPLACE INTO schema_migrations (version, applied_at) VALUES
    ('20260419-000000_add_edges.sql', datetime('now')),
    ('20260419-000100_add_edge_source.sql', datetime('now')),
    ('20260419-000100_add_task_acceptance_criteria.sql', datetime('now')),
    ('20260419-000200_fix_edge_rel_type_references.sql', datetime('now')),
    ('20260419-000300_add_edge_valid_until.sql', datetime('now')),
    ('20260420-000000_add_missing_indexes.sql', datetime('now')),
    ('20260420-000100_add_users.sql', datetime('now')),
    ('20260420-000400_add_message_refs.sql', datetime('now')),
    ('20260420-000500_migrate_relations_to_edges.sql', datetime('now')),
    ('20260420-000600_remove_legacy_relation_fields.sql', datetime('now')),
    ('20260420-000700_add_edge_indexes.sql', datetime('now'));

-- ============================================================================
-- Verification queries (commented out - run manually to verify)
-- ============================================================================
-- SELECT 'Edges created:' as check_name, COUNT(*) as count FROM edges;
-- SELECT 'Tasks with parent_id:' as check_name, COUNT(*) as count FROM tasks WHERE parent_id IS NOT NULL;
-- SELECT 'Knowledge with agent_id:' as check_name, COUNT(*) as count FROM knowledge_entries WHERE agent_id IS NOT NULL;
-- SELECT 'Agents with parent_id:' as check_name, COUNT(*) as count FROM agents WHERE parent_id IS NOT NULL;
