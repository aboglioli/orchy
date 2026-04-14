CREATE TABLE IF NOT EXISTS project_links (
    id TEXT PRIMARY KEY,
    source_project TEXT NOT NULL,
    target_project TEXT NOT NULL,
    resource_types TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL,
    UNIQUE(source_project, target_project)
);
