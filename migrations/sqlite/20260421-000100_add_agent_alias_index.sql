-- Add unique index on agents (organization_id, project, alias)
-- Agents with empty-string aliases from before alias support was added
-- can violate the unique constraint if multiple agents share the same
-- (organization_id, project). Assign them unique aliases based on their ID.

UPDATE agents SET alias = 'agent-' || id WHERE alias = '';

CREATE UNIQUE INDEX IF NOT EXISTS idx_agents_alias_unique ON agents (organization_id, project, alias);