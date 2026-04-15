-- Consolidated initial schema for PostgreSQL
-- Date: 2026-04-15

CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE IF NOT EXISTS agents (
    id UUID PRIMARY KEY,
    project TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT '/',
    parent_id UUID REFERENCES agents(id),
    alias TEXT,
    roles JSONB NOT NULL DEFAULT '[]',
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'online',
    last_heartbeat TIMESTAMPTZ NOT NULL,
    connected_at TIMESTAMPTZ NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'
);
CREATE UNIQUE INDEX IF NOT EXISTS agents_project_alias_idx ON agents (project, alias) WHERE alias IS NOT NULL;

CREATE TABLE IF NOT EXISTS tasks (
    id UUID PRIMARY KEY,
    project TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT '/',
    parent_id UUID REFERENCES tasks(id),
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending',
    priority TEXT NOT NULL DEFAULT 'normal',
    assigned_roles JSONB NOT NULL DEFAULT '[]',
    assigned_to UUID REFERENCES agents(id),
    assigned_at TIMESTAMPTZ,
    depends_on JSONB NOT NULL DEFAULT '[]',
    tags JSONB NOT NULL DEFAULT '[]',
    result_summary TEXT,
    notes JSONB NOT NULL DEFAULT '[]',
    created_by UUID REFERENCES agents(id),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS messages (
    id UUID PRIMARY KEY,
    project TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT '/',
    from_agent UUID NOT NULL REFERENCES agents(id),
    to_target TEXT NOT NULL,
    body TEXT NOT NULL,
    reply_to UUID REFERENCES messages(id),
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS projects (
    name TEXT PRIMARY KEY,
    description TEXT NOT NULL DEFAULT '',
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS namespaces (
    project TEXT NOT NULL,
    namespace TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (project, namespace)
);

CREATE TABLE IF NOT EXISTS knowledge_entries (
    id UUID PRIMARY KEY,
    project TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT '/',
    path TEXT NOT NULL,
    kind TEXT NOT NULL,
    title TEXT NOT NULL DEFAULT '',
    content TEXT NOT NULL DEFAULT '',
    tags JSONB NOT NULL DEFAULT '[]',
    version BIGINT NOT NULL DEFAULT 1,
    agent_id UUID REFERENCES agents(id),
    metadata JSONB NOT NULL DEFAULT '{}',
    embedding VECTOR,
    embedding_model TEXT,
    embedding_dimensions INTEGER,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE(project, namespace, path)
);
CREATE INDEX IF NOT EXISTS knowledge_entries_type_idx ON knowledge_entries (kind);
CREATE INDEX IF NOT EXISTS knowledge_entries_agent_idx ON knowledge_entries (agent_id);

CREATE TABLE IF NOT EXISTS resource_locks (
    project TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT '/',
    name TEXT NOT NULL,
    holder UUID NOT NULL REFERENCES agents(id),
    acquired_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (project, namespace, name)
);

CREATE TABLE IF NOT EXISTS task_watchers (
    task_id UUID NOT NULL REFERENCES tasks(id),
    agent_id UUID NOT NULL REFERENCES agents(id),
    project TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT '/',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (task_id, agent_id)
);

CREATE TABLE IF NOT EXISTS reviews (
    id UUID PRIMARY KEY,
    task_id UUID NOT NULL REFERENCES tasks(id),
    project TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT '/',
    requester UUID NOT NULL REFERENCES agents(id),
    reviewer UUID REFERENCES agents(id),
    reviewer_role TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    comments TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved_at TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS reviews_task_idx ON reviews (task_id);
CREATE INDEX IF NOT EXISTS reviews_status_idx ON reviews (status);

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

CREATE TABLE IF NOT EXISTS project_links (
    id UUID PRIMARY KEY,
    source_project TEXT NOT NULL,
    target_project TEXT NOT NULL,
    resource_types JSONB NOT NULL DEFAULT '[]',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(source_project, target_project)
);
