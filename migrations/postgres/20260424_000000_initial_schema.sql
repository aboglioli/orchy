-- Consolidated initial schema for PostgreSQL
-- Generated: 2026-04-24
-- Merges all migrations from 20260415 through 20260423

CREATE EXTENSION IF NOT EXISTS vector;

-- Schema migrations tracking
CREATE TABLE IF NOT EXISTS schema_migrations (
    version TEXT PRIMARY KEY,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Organizations
CREATE TABLE IF NOT EXISTS organizations (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Users
CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY,
    email TEXT UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    is_platform_admin BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_users_email ON users (email);

-- Organization memberships
CREATE TABLE IF NOT EXISTS org_memberships (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    UNIQUE (user_id, org_id)
);

CREATE INDEX IF NOT EXISTS idx_memberships_user ON org_memberships (user_id);
CREATE INDEX IF NOT EXISTS idx_memberships_org ON org_memberships (org_id);

-- API keys
CREATE TABLE IF NOT EXISTS api_keys (
    id UUID PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    name TEXT NOT NULL DEFAULT '',
    key TEXT NOT NULL UNIQUE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS api_keys_organization_idx ON api_keys (organization_id);

-- Agents
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
    metadata JSONB NOT NULL DEFAULT '{}',
    user_id UUID REFERENCES users(id) ON DELETE SET NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_agents_alias_unique ON agents (organization_id, project, alias);
CREATE INDEX IF NOT EXISTS idx_agents_status_last_seen ON agents (status, last_seen);

-- Projects
CREATE TABLE IF NOT EXISTS projects (
    organization_id TEXT NOT NULL DEFAULT 'default',
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (organization_id, name)
);

-- Namespaces
CREATE TABLE IF NOT EXISTS namespaces (
    organization_id TEXT NOT NULL DEFAULT 'default',
    project TEXT NOT NULL,
    namespace TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (organization_id, project, namespace)
);

-- Tasks
CREATE TABLE IF NOT EXISTS tasks (
    id UUID PRIMARY KEY,
    organization_id TEXT NOT NULL DEFAULT 'default',
    project TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT '/',
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    acceptance_criteria TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    priority TEXT NOT NULL DEFAULT 'normal',
    assigned_roles JSONB NOT NULL DEFAULT '[]',
    assigned_to UUID REFERENCES agents(id),
    assigned_at TIMESTAMPTZ,
    tags JSONB NOT NULL DEFAULT '[]',
    result_summary TEXT,
    stale_after_secs BIGINT,
    last_activity_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID REFERENCES agents(id),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    archived_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_tasks_assigned_to ON tasks (assigned_to);
CREATE INDEX IF NOT EXISTS idx_tasks_status_priority ON tasks (status, priority);
CREATE INDEX IF NOT EXISTS tasks_assigned_roles_gin_idx ON tasks USING gin (assigned_roles jsonb_path_ops);
CREATE INDEX IF NOT EXISTS idx_tasks_archived ON tasks (organization_id, archived_at) WHERE archived_at IS NOT NULL;

-- Knowledge entries
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
    valid_from TIMESTAMPTZ,
    valid_until TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    archived_at TIMESTAMPTZ
);

CREATE UNIQUE INDEX IF NOT EXISTS knowledge_entries_project_path_idx
    ON knowledge_entries (organization_id, project, namespace, path)
    WHERE project IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS knowledge_entries_org_path_idx
    ON knowledge_entries (organization_id, namespace, path)
    WHERE project IS NULL;
CREATE INDEX IF NOT EXISTS knowledge_entries_type_idx ON knowledge_entries (kind);
CREATE INDEX IF NOT EXISTS knowledge_entries_search_idx
    ON knowledge_entries USING gin (to_tsvector('english', title || ' ' || content));
CREATE INDEX IF NOT EXISTS idx_knowledge_archived
    ON knowledge_entries (organization_id, archived_at) WHERE archived_at IS NOT NULL;

-- Messages
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
    claimed_by UUID REFERENCES agents(id) ON DELETE SET NULL,
    claimed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_messages_to_target ON messages (to_target);

-- Message receipts
CREATE TABLE IF NOT EXISTS message_receipts (
    message_id UUID NOT NULL REFERENCES messages(id),
    agent_id UUID NOT NULL REFERENCES agents(id),
    read_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (message_id, agent_id)
);

-- Resource locks
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

-- Events
CREATE TABLE IF NOT EXISTS events (
    id UUID PRIMARY KEY,
    seq BIGSERIAL,
    organization TEXT NOT NULL,
    namespace TEXT NOT NULL,
    topic TEXT NOT NULL,
    payload JSONB NOT NULL,
    content_type TEXT NOT NULL DEFAULT 'application/json',
    metadata JSONB NOT NULL DEFAULT '{}',
    timestamp TIMESTAMPTZ NOT NULL,
    version BIGINT NOT NULL DEFAULT 1
);

CREATE UNIQUE INDEX IF NOT EXISTS events_seq_idx ON events (seq);
CREATE INDEX IF NOT EXISTS events_topic_idx ON events (topic);
CREATE INDEX IF NOT EXISTS events_namespace_idx ON events (namespace);
CREATE INDEX IF NOT EXISTS events_timestamp_idx ON events (timestamp);
CREATE INDEX IF NOT EXISTS events_organization_idx ON events (organization);
CREATE INDEX IF NOT EXISTS idx_events_org_timestamp ON events (organization, timestamp);

-- Events NOTIFY trigger
CREATE OR REPLACE FUNCTION notify_event_inserted()
RETURNS TRIGGER AS $$
BEGIN
    PERFORM pg_notify('orchy_events', NEW.organization);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER events_after_insert
AFTER INSERT ON events
FOR EACH ROW EXECUTE FUNCTION notify_event_inserted();

-- Edges (graph layer)
CREATE TABLE IF NOT EXISTS edges (
    id UUID PRIMARY KEY,
    org_id TEXT NOT NULL,
    from_kind TEXT NOT NULL,
    from_id TEXT NOT NULL,
    to_kind TEXT NOT NULL,
    to_id TEXT NOT NULL,
    rel_type TEXT NOT NULL,
    display TEXT,
    source_kind TEXT,
    source_id TEXT,
    valid_until TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    created_by UUID
);

CREATE INDEX IF NOT EXISTS idx_edges_from ON edges (org_id, from_kind, from_id);
CREATE INDEX IF NOT EXISTS idx_edges_to ON edges (org_id, to_kind, to_id);
CREATE INDEX IF NOT EXISTS idx_edges_rel_type ON edges (org_id, rel_type);
CREATE INDEX IF NOT EXISTS idx_edges_valid_until ON edges (org_id, valid_until);
CREATE UNIQUE INDEX IF NOT EXISTS idx_edges_unique_active
    ON edges (org_id, from_kind, from_id, to_kind, to_id, rel_type)
    WHERE valid_until IS NULL;
CREATE INDEX IF NOT EXISTS idx_edges_outgoing
    ON edges (org_id, from_kind, from_id, rel_type, to_kind, to_id, created_at)
    WHERE valid_until IS NULL;
CREATE INDEX IF NOT EXISTS idx_edges_incoming
    ON edges (org_id, to_kind, to_id, rel_type, from_kind, from_id, created_at)
    WHERE valid_until IS NULL;

-- Consumer offsets
CREATE TABLE IF NOT EXISTS consumer_offsets (
    group_id TEXT PRIMARY KEY,
    last_seq BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
