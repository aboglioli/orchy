CREATE TABLE IF NOT EXISTS skills_new (
    project TEXT NOT NULL,
    namespace TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    content TEXT NOT NULL,
    written_by TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (project, namespace, name)
);

INSERT OR IGNORE INTO skills_new SELECT project, namespace, name, description, content, written_by, created_at, updated_at FROM skills;
DROP TABLE skills;
ALTER TABLE skills_new RENAME TO skills;
