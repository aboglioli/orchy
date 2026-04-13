CREATE TABLE IF NOT EXISTS agents (
    id TEXT PRIMARY KEY,
    project TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT '/',
    parent_id TEXT,
    roles TEXT NOT NULL DEFAULT '[]',
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'online',
    last_heartbeat TEXT NOT NULL,
    connected_at TEXT NOT NULL,
    metadata TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    project TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT '/',
    parent_id TEXT,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending',
    priority TEXT NOT NULL DEFAULT 'normal',
    assigned_roles TEXT NOT NULL DEFAULT '[]',
    assigned_to TEXT,
    assigned_at TEXT,
    depends_on TEXT NOT NULL DEFAULT '[]',
    result_summary TEXT,
    notes TEXT NOT NULL DEFAULT '[]',
    created_by TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS memory (
    project TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT '/',
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    embedding BLOB,
    embedding_model TEXT,
    embedding_dimensions INTEGER,
    written_by TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (project, namespace, key)
);

CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    project TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT '/',
    from_agent TEXT NOT NULL,
    to_target TEXT NOT NULL,
    body TEXT NOT NULL,
    reply_to TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS contexts (
    id TEXT PRIMARY KEY,
    project TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT '/',
    summary TEXT NOT NULL,
    embedding BLOB,
    embedding_model TEXT,
    embedding_dimensions INTEGER,
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS skills (
    project TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT '/',
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    content TEXT NOT NULL,
    written_by TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (project, namespace, name)
);

CREATE TABLE IF NOT EXISTS projects (
    name TEXT PRIMARY KEY,
    description TEXT NOT NULL DEFAULT '',
    notes TEXT NOT NULL DEFAULT '[]',
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS namespaces (
    project TEXT NOT NULL,
    namespace TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (project, namespace)
);
