-- Consolidated initial schema for PostgreSQL
-- Date: 2026-04-15

CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE IF NOT EXISTS organizations (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS api_keys (
    id UUID PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    name TEXT NOT NULL DEFAULT '',
    key TEXT NOT NULL UNIQUE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS api_keys_organization_idx ON api_keys (organization_id);

CREATE TABLE IF NOT EXISTS agents (
    id UUID PRIMARY KEY,
    alias TEXT NOT NULL DEFAULT '',
    organization_id TEXT NOT NULL DEFAULT 'default',
    project TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT '/',
    roles JSONB NOT NULL DEFAULT '[]',
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'online',
    last_seen TIMESTAMPTZ NOT NULL,
    connected_at TIMESTAMPTZ NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS tasks (
    id UUID PRIMARY KEY,
    organization_id TEXT NOT NULL DEFAULT 'default',
    project TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT '/',
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending',
    priority TEXT NOT NULL DEFAULT 'normal',
    assigned_roles JSONB NOT NULL DEFAULT '[]',
    assigned_to UUID REFERENCES agents(id),
    assigned_at TIMESTAMPTZ,
    tags JSONB NOT NULL DEFAULT '[]',
    result_summary TEXT,
    created_by UUID REFERENCES agents(id),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS messages (
    id UUID PRIMARY KEY,
    organization_id TEXT NOT NULL DEFAULT 'default',
    project TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT '/',
    from_agent UUID NOT NULL REFERENCES agents(id),
    to_target TEXT NOT NULL,
    body TEXT NOT NULL,
    reply_to UUID REFERENCES messages(id),
    refs JSONB NOT NULL DEFAULT '[]',
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS message_receipts (
    message_id UUID NOT NULL REFERENCES messages(id),
    agent_id UUID NOT NULL REFERENCES agents(id),
    read_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (message_id, agent_id)
);

CREATE TABLE IF NOT EXISTS projects (
    organization_id TEXT NOT NULL DEFAULT 'default',
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (organization_id, name)
);

CREATE TABLE IF NOT EXISTS namespaces (
    organization_id TEXT NOT NULL DEFAULT 'default',
    project TEXT NOT NULL,
    namespace TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (organization_id, project, namespace)
);

CREATE TABLE IF NOT EXISTS knowledge_entries (
    id UUID PRIMARY KEY,
    organization_id TEXT NOT NULL DEFAULT 'default',
    project TEXT,
    namespace TEXT NOT NULL DEFAULT '/',
    path TEXT NOT NULL,
    kind TEXT NOT NULL,
    title TEXT NOT NULL DEFAULT '',
    content TEXT NOT NULL DEFAULT '',
    tags JSONB NOT NULL DEFAULT '[]',
    version BIGINT NOT NULL DEFAULT 1,
    metadata JSONB NOT NULL DEFAULT '{}',
    embedding VECTOR,
    embedding_model TEXT,
    embedding_dimensions INTEGER,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS knowledge_entries_project_path_idx
    ON knowledge_entries (organization_id, project, namespace, path)
    WHERE project IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS knowledge_entries_org_path_idx
    ON knowledge_entries (organization_id, namespace, path)
    WHERE project IS NULL;
CREATE INDEX IF NOT EXISTS knowledge_entries_type_idx ON knowledge_entries (kind);

CREATE TABLE IF NOT EXISTS resource_locks (
    organization_id TEXT NOT NULL DEFAULT 'default',
    project TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT '/',
    name TEXT NOT NULL,
    holder UUID NOT NULL REFERENCES agents(id),
    acquired_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (organization_id, project, namespace, name)
);

CREATE TABLE IF NOT EXISTS events (
    id UUID PRIMARY KEY,
    organization TEXT NOT NULL,
    namespace TEXT NOT NULL,
    topic TEXT NOT NULL,
    payload JSONB NOT NULL,
    content_type TEXT NOT NULL DEFAULT 'application/json',
    metadata JSONB NOT NULL DEFAULT '{}',
    timestamp TIMESTAMPTZ NOT NULL,
    version BIGINT NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS events_topic_idx ON events (topic);
CREATE INDEX IF NOT EXISTS events_namespace_idx ON events (namespace);
CREATE INDEX IF NOT EXISTS events_timestamp_idx ON events (timestamp);
CREATE INDEX IF NOT EXISTS events_organization_idx ON events (organization);
