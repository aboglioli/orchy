CREATE TABLE IF NOT EXISTS edges (
    id UUID PRIMARY KEY,
    org_id TEXT NOT NULL,
    from_kind TEXT NOT NULL,
    from_id TEXT NOT NULL,
    to_kind TEXT NOT NULL,
    to_id TEXT NOT NULL,
    rel_type TEXT NOT NULL,
    display TEXT,
    created_at TIMESTAMPTZ NOT NULL,
    created_by TEXT
);

CREATE INDEX IF NOT EXISTS idx_edges_from ON edges (org_id, from_kind, from_id);
CREATE INDEX IF NOT EXISTS idx_edges_to ON edges (org_id, to_kind, to_id);
CREATE INDEX IF NOT EXISTS idx_edges_rel_type ON edges (org_id, rel_type);

-- Migrate existing knowledge refs into edges table
INSERT INTO edges (id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at)
SELECT
    gen_random_uuid() AS id,
    organization_id AS org_id,
    'knowledge' AS from_kind,
    id AS from_id,
    r->>'kind' AS to_kind,
    r->>'id' AS to_id,
    'related_to' AS rel_type,
    r->>'display' AS display,
    updated_at AS created_at
FROM knowledge_entries,
     jsonb_array_elements(COALESCE(refs, '[]'::jsonb)) AS r
WHERE r->>'id' IS NOT NULL
ON CONFLICT DO NOTHING;

-- Migrate existing task refs into edges table
INSERT INTO edges (id, org_id, from_kind, from_id, to_kind, to_id, rel_type, display, created_at)
SELECT
    gen_random_uuid() AS id,
    organization_id AS org_id,
    'task' AS from_kind,
    id AS from_id,
    r->>'kind' AS to_kind,
    r->>'id' AS to_id,
    'related_to' AS rel_type,
    r->>'display' AS display,
    updated_at AS created_at
FROM tasks,
     jsonb_array_elements(COALESCE(refs, '[]'::jsonb)) AS r
WHERE r->>'id' IS NOT NULL
ON CONFLICT DO NOTHING;
