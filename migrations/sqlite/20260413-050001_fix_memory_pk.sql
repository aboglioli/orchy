CREATE TABLE IF NOT EXISTS memory_new (
    project TEXT NOT NULL,
    namespace TEXT NOT NULL,
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

INSERT OR IGNORE INTO memory_new SELECT project, namespace, key, value, version, embedding, embedding_model, embedding_dimensions, written_by, created_at, updated_at FROM memory;
DROP TABLE memory;
ALTER TABLE memory_new RENAME TO memory;
