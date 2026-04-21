-- Consolidated initial schema for SQLite

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

CREATE TABLE IF NOT EXISTS agents (
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

CREATE TABLE IF NOT EXISTS tasks (
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
    stale_after_secs INTEGER,
    last_activity_at TEXT NOT NULL,
    tags TEXT NOT NULL DEFAULT '[]',
    result_summary TEXT,
    created_by TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL DEFAULT 'default',
    project TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT '/',
    from_agent TEXT NOT NULL,
    to_target TEXT NOT NULL,
    body TEXT NOT NULL,
    reply_to TEXT,
    refs TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS message_receipts (
    message_id TEXT NOT NULL REFERENCES messages(id),
    agent_id TEXT NOT NULL,
    read_at TEXT NOT NULL,
    PRIMARY KEY (message_id, agent_id)
);

CREATE TABLE IF NOT EXISTS projects (
    organization_id TEXT NOT NULL DEFAULT 'default',
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (organization_id, name)
);

CREATE TABLE IF NOT EXISTS namespaces (
    organization_id TEXT NOT NULL DEFAULT 'default',
    project TEXT NOT NULL,
    namespace TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (organization_id, project, namespace)
);

CREATE TABLE IF NOT EXISTS knowledge_entries (
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
CREATE UNIQUE INDEX IF NOT EXISTS knowledge_entries_project_path_idx
    ON knowledge_entries (organization_id, project, namespace, path)
    WHERE project IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS knowledge_entries_org_path_idx
    ON knowledge_entries (organization_id, namespace, path)
    WHERE project IS NULL;
CREATE INDEX IF NOT EXISTS knowledge_entries_type_idx ON knowledge_entries (kind);

CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_entries_fts USING fts5(
    knowledge_id UNINDEXED,
    path,
    title,
    content,
    tokenize = 'porter'
);

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

CREATE TABLE IF NOT EXISTS resource_locks (
    organization_id TEXT NOT NULL DEFAULT 'default',
    project TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT '/',
    name TEXT NOT NULL,
    holder TEXT NOT NULL,
    acquired_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    PRIMARY KEY (organization_id, project, namespace, name)
);

CREATE TABLE IF NOT EXISTS events (
    id TEXT PRIMARY KEY,
    organization TEXT NOT NULL,
    namespace TEXT NOT NULL,
    topic TEXT NOT NULL,
    payload TEXT NOT NULL,
    content_type TEXT NOT NULL DEFAULT 'application/json',
    metadata TEXT NOT NULL DEFAULT '{}',
    timestamp TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS events_topic_idx ON events (topic);
CREATE INDEX IF NOT EXISTS events_namespace_idx ON events (namespace);
CREATE INDEX IF NOT EXISTS events_timestamp_idx ON events (timestamp);
CREATE INDEX IF NOT EXISTS events_organization_idx ON events (organization);

-- Edges table for graph relationships
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
    created_by TEXT
);
CREATE INDEX IF NOT EXISTS idx_edges_from ON edges (org_id, from_kind, from_id);
CREATE INDEX IF NOT EXISTS idx_edges_to ON edges (org_id, to_kind, to_id);
CREATE INDEX IF NOT EXISTS idx_edges_rel_type ON edges (org_id, rel_type);
