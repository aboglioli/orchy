-- Incremental migration: add multi-org tenancy to existing database
-- Preserves all existing data, assigns everything to the 'default' organization.

-- organizations and api_keys tables
CREATE TABLE IF NOT EXISTS organizations (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS api_keys (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    name TEXT NOT NULL DEFAULT '',
    key TEXT NOT NULL UNIQUE,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS api_keys_organization_idx ON api_keys (organization_id);

-- Seed the default organization
INSERT OR IGNORE INTO organizations (id, name, created_at, updated_at)
VALUES ('default', 'Default', datetime('now'), datetime('now'));

-- Add organization_id to simple tables (ALTER ADD COLUMN is safe in SQLite)
ALTER TABLE agents ADD COLUMN organization_id TEXT NOT NULL DEFAULT 'default';
ALTER TABLE tasks ADD COLUMN organization_id TEXT NOT NULL DEFAULT 'default';
ALTER TABLE messages ADD COLUMN organization_id TEXT NOT NULL DEFAULT 'default';
ALTER TABLE task_watchers ADD COLUMN organization_id TEXT NOT NULL DEFAULT 'default';
ALTER TABLE reviews ADD COLUMN organization_id TEXT NOT NULL DEFAULT 'default';

-- Drop old unique index on agents; recreate with org scope
DROP INDEX IF EXISTS agents_project_alias_idx;
CREATE UNIQUE INDEX IF NOT EXISTS agents_project_alias_idx ON agents (organization_id, project, alias) WHERE alias IS NOT NULL;

-- knowledge_entries: add organization_id, make project nullable, update unique constraint
-- SQLite cannot ALTER column nullability, so rebuild the table.
PRAGMA foreign_keys = OFF;

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
    agent_id TEXT,
    metadata TEXT NOT NULL DEFAULT '{}',
    embedding BLOB,
    embedding_model TEXT,
    embedding_dimensions INTEGER,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

INSERT INTO knowledge_entries_new
    (id, organization_id, project, namespace, path, kind, title, content, tags, version,
     agent_id, metadata, embedding, embedding_model, embedding_dimensions, created_at, updated_at)
SELECT id, 'default', project, namespace, path, kind, title, content, tags, version,
       agent_id, metadata, embedding, embedding_model, embedding_dimensions, created_at, updated_at
FROM knowledge_entries;

DROP TRIGGER IF EXISTS trg_knowledge_entries_ai;
DROP TRIGGER IF EXISTS trg_knowledge_entries_ad;
DROP TRIGGER IF EXISTS trg_knowledge_entries_au;

DROP TABLE knowledge_entries;
ALTER TABLE knowledge_entries_new RENAME TO knowledge_entries;

CREATE UNIQUE INDEX IF NOT EXISTS knowledge_entries_project_path_idx
    ON knowledge_entries (organization_id, project, namespace, path)
    WHERE project IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS knowledge_entries_org_path_idx
    ON knowledge_entries (organization_id, namespace, path)
    WHERE project IS NULL;
CREATE INDEX IF NOT EXISTS knowledge_entries_type_idx ON knowledge_entries (kind);
CREATE INDEX IF NOT EXISTS knowledge_entries_agent_idx ON knowledge_entries (agent_id);

CREATE TRIGGER trg_knowledge_entries_ai AFTER INSERT ON knowledge_entries BEGIN
    INSERT INTO knowledge_entries_fts(knowledge_id, path, title, content)
    VALUES (new.id, new.path, new.title, new.content);
END;
CREATE TRIGGER trg_knowledge_entries_ad AFTER DELETE ON knowledge_entries BEGIN
    DELETE FROM knowledge_entries_fts WHERE knowledge_id = old.id;
END;
CREATE TRIGGER trg_knowledge_entries_au AFTER UPDATE ON knowledge_entries BEGIN
    UPDATE knowledge_entries_fts
    SET path = new.path, title = new.title, content = new.content
    WHERE knowledge_id = old.id;
END;

-- Rebuild namespaces with new PK (organization_id, project, namespace)
CREATE TABLE namespaces_new (
    organization_id TEXT NOT NULL DEFAULT 'default',
    project TEXT NOT NULL,
    namespace TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (organization_id, project, namespace)
);
INSERT INTO namespaces_new (organization_id, project, namespace, created_at)
SELECT 'default', project, namespace, created_at FROM namespaces;
DROP TABLE namespaces;
ALTER TABLE namespaces_new RENAME TO namespaces;

-- Rebuild resource_locks with new PK (organization_id, project, namespace, name)
CREATE TABLE resource_locks_new (
    organization_id TEXT NOT NULL DEFAULT 'default',
    project TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT '/',
    name TEXT NOT NULL,
    holder TEXT NOT NULL,
    acquired_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    PRIMARY KEY (organization_id, project, namespace, name)
);
INSERT INTO resource_locks_new (organization_id, project, namespace, name, holder, acquired_at, expires_at)
SELECT 'default', project, namespace, name, holder, acquired_at, expires_at FROM resource_locks;
DROP TABLE resource_locks;
ALTER TABLE resource_locks_new RENAME TO resource_locks;

-- Rebuild projects with new PK (organization_id, name)
CREATE TABLE projects_new (
    organization_id TEXT NOT NULL DEFAULT 'default',
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (organization_id, name)
);
INSERT INTO projects_new (organization_id, name, description, metadata, created_at, updated_at)
SELECT 'default', name, description, metadata, created_at, updated_at FROM projects;
DROP TABLE projects;
ALTER TABLE projects_new RENAME TO projects;

-- Drop obsolete project_links table if it exists
DROP TABLE IF EXISTS project_links;

PRAGMA foreign_keys = ON;
