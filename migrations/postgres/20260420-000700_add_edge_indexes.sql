CREATE UNIQUE INDEX IF NOT EXISTS idx_edges_unique_active
    ON edges (org_id, from_kind, from_id, to_kind, to_id, rel_type)
    WHERE valid_until IS NULL;

CREATE INDEX IF NOT EXISTS idx_edges_outgoing
    ON edges (org_id, from_kind, from_id, rel_type, to_kind, to_id, created_at)
    WHERE valid_until IS NULL;

CREATE INDEX IF NOT EXISTS idx_edges_incoming
    ON edges (org_id, to_kind, to_id, rel_type, from_kind, from_id, created_at)
    WHERE valid_until IS NULL;