CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE IF NOT EXISTS agents (
    id UUID PRIMARY KEY,
    project TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT '/',
    parent_id UUID REFERENCES agents(id),
    roles JSONB NOT NULL DEFAULT '[]',
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'online',
    last_heartbeat TIMESTAMPTZ NOT NULL,
    connected_at TIMESTAMPTZ NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'
);

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
    result_summary TEXT,
    notes JSONB NOT NULL DEFAULT '[]',
    created_by UUID REFERENCES agents(id),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS memory (
    project TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT '/',
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    version BIGINT NOT NULL DEFAULT 1,
    embedding VECTOR,
    embedding_model TEXT,
    embedding_dimensions INTEGER,
    written_by UUID REFERENCES agents(id),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (project, namespace, key)
);

CREATE INDEX IF NOT EXISTS memory_fts_idx ON memory USING gin(to_tsvector('english', value));

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

CREATE TABLE IF NOT EXISTS contexts (
    id UUID PRIMARY KEY,
    project TEXT NOT NULL,
    agent_id UUID NOT NULL REFERENCES agents(id),
    namespace TEXT NOT NULL DEFAULT '/',
    summary TEXT NOT NULL,
    embedding VECTOR,
    embedding_model TEXT,
    embedding_dimensions INTEGER,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS contexts_fts_idx ON contexts USING gin(to_tsvector('english', summary));

CREATE TABLE IF NOT EXISTS skills (
    project TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT '/',
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    content TEXT NOT NULL,
    written_by UUID REFERENCES agents(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (project, namespace, name)
);

CREATE TABLE IF NOT EXISTS projects (
    name TEXT PRIMARY KEY,
    description TEXT NOT NULL DEFAULT '',
    notes JSONB NOT NULL DEFAULT '[]',
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
