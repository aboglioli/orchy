ALTER TABLE edges ADD COLUMN valid_until TEXT;
CREATE INDEX IF NOT EXISTS idx_edges_valid_until ON edges (org_id, valid_until);
