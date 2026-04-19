ALTER TABLE edges ADD COLUMN IF NOT EXISTS valid_until TIMESTAMPTZ;
CREATE INDEX IF NOT EXISTS idx_edges_valid_until ON edges (org_id, valid_until);
