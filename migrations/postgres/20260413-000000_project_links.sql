CREATE TABLE IF NOT EXISTS project_links (
    id UUID PRIMARY KEY,
    source_project TEXT NOT NULL,
    target_project TEXT NOT NULL,
    resource_types JSONB NOT NULL DEFAULT '[]',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(source_project, target_project)
);
