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

-- Migrate existing knowledge refs into edges table
INSERT OR IGNORE INTO edges (id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at)
SELECT
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-7' ||
    substr(lower(hex(randomblob(2))), 2) || '-' || lower(hex(randomblob(2))) || '-' ||
    lower(hex(randomblob(6))) AS id,
    ke.organization_id AS org_id,
    'knowledge' AS from_kind,
    ke.id AS from_id,
    json_extract(r.value, '$.kind') AS to_kind,
    json_extract(r.value, '$.id') AS to_id,
    'related_to' AS rel_type,
    json_extract(r.value, '$.display') AS display,
    ke.updated_at AS created_at
FROM knowledge_entries ke,
     json_each(COALESCE(ke.refs, '[]')) AS r
WHERE json_extract(r.value, '$.id') IS NOT NULL;

-- Migrate existing task refs into edges table
INSERT OR IGNORE INTO edges (id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at)
SELECT
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-7' ||
    substr(lower(hex(randomblob(2))), 2) || '-' || lower(hex(randomblob(2))) || '-' ||
    lower(hex(randomblob(6))) AS id,
    t.organization_id AS org_id,
    'task' AS from_kind,
    t.id AS from_id,
    json_extract(r.value, '$.kind') AS to_kind,
    json_extract(r.value, '$.id') AS to_id,
    'related_to' AS rel_type,
    json_extract(r.value, '$.display') AS display,
    t.updated_at AS created_at
FROM tasks t,
     json_each(COALESCE(t.refs, '[]')) AS r
WHERE json_extract(r.value, '$.id') IS NOT NULL;
