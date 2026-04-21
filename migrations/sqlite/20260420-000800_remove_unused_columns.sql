-- Remove unused columns that are not in domain models
-- SQLite doesn't support DROP COLUMN directly, so we recreate tables

-- Remove notes from tasks (not in Task model)
CREATE TABLE tasks_new (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL DEFAULT 'default',
    project TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT '/',
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending',
    priority TEXT NOT NULL DEFAULT 'normal',
    assigned_roles TEXT NOT NULL DEFAULT '[]',
    assigned_to TEXT,
    assigned_at TEXT,
    acceptance_criteria TEXT,
    stale_after_secs INTEGER,
    last_activity_at TEXT NOT NULL,
    tags TEXT NOT NULL DEFAULT '[]',
    result_summary TEXT,
    created_by TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
INSERT INTO tasks_new SELECT
    id, organization_id, project, namespace, title, description, status, priority,
    assigned_roles, assigned_to, assigned_at, acceptance_criteria, stale_after_secs,
    last_activity_at, tags, result_summary, created_by, created_at, updated_at
FROM tasks;
DROP TABLE tasks;
ALTER TABLE tasks_new RENAME TO tasks;

-- Remove refs from knowledge_entries (not in Knowledge model)
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
    updated_at TEXT NOT NULL
);
INSERT INTO knowledge_entries_new SELECT
    id, organization_id, project, namespace, path, kind, title, content,
    tags, version, metadata, embedding, embedding_model, embedding_dimensions,
    created_at, updated_at
FROM knowledge_entries;
DROP TABLE knowledge_entries;
ALTER TABLE knowledge_entries_new RENAME TO knowledge_entries;

-- Recreate indexes
CREATE INDEX IF NOT EXISTS idx_agents_status_heartbeat ON agents (status, last_heartbeat);
CREATE INDEX IF NOT EXISTS idx_tasks_assigned_to ON tasks (assigned_to);
CREATE INDEX IF NOT EXISTS idx_tasks_status_priority ON tasks (status, priority);
CREATE UNIQUE INDEX IF NOT EXISTS knowledge_entries_project_path_idx
    ON knowledge_entries (organization_id, project, namespace, path)
    WHERE project IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS knowledge_entries_org_path_idx
    ON knowledge_entries (organization_id, namespace, path)
    WHERE project IS NULL;
CREATE INDEX IF NOT EXISTS knowledge_entries_type_idx ON knowledge_entries (kind);

-- Recreate FTS triggers
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
