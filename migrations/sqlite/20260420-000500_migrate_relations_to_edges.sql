-- Phase 2 data migration: relational fields → edges
-- Apply manually after Phase 1 deployment is stable.
-- Uses INSERT OR IGNORE for idempotency (requires a unique constraint on edges
-- to be effective; otherwise run once only).

-- Task parent-child → Spawns edges (parent spawned child)
INSERT OR IGNORE INTO edges (id, org_id, from_kind, from_id, to_kind, to_id, rel_type, created_at)
SELECT
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-7' ||
    substr(lower(hex(randomblob(2))), 2) || '-' || lower(hex(randomblob(2))) || '-' ||
    lower(hex(randomblob(6))),
    organization_id,
    'task',
    CAST(parent_id AS TEXT),
    'task',
    CAST(id AS TEXT),
    'spawns',
    created_at
FROM tasks
WHERE parent_id IS NOT NULL;

-- Task dependencies → DependsOn edges
INSERT OR IGNORE INTO edges (id, org_id, from_kind, from_id, to_kind, to_id, rel_type, created_at)
SELECT
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-7' ||
    substr(lower(hex(randomblob(2))), 2) || '-' || lower(hex(randomblob(2))) || '-' ||
    lower(hex(randomblob(6))),
    t.organization_id,
    'task',
    CAST(t.id AS TEXT),
    'task',
    json_each.value,
    'depends_on',
    t.created_at
FROM tasks t, json_each(t.depends_on)
WHERE t.depends_on IS NOT NULL
  AND t.depends_on != '[]'
  AND t.depends_on != '';

-- Knowledge authorship → OwnedBy edges
INSERT OR IGNORE INTO edges (id, org_id, from_kind, from_id, to_kind, to_id, rel_type, created_at)
SELECT
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-7' ||
    substr(lower(hex(randomblob(2))), 2) || '-' || lower(hex(randomblob(2))) || '-' ||
    lower(hex(randomblob(6))),
    organization_id,
    'knowledge',
    CAST(id AS TEXT),
    'agent',
    CAST(agent_id AS TEXT),
    'owned_by',
    created_at
FROM knowledge_entries
WHERE agent_id IS NOT NULL;

-- Agent parent hierarchy → Spawns edges (parent spawned child agent)
INSERT OR IGNORE INTO edges (id, org_id, from_kind, from_id, to_kind, to_id, rel_type, created_at)
SELECT
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-7' ||
    substr(lower(hex(randomblob(2))), 2) || '-' || lower(hex(randomblob(2))) || '-' ||
    lower(hex(randomblob(6))),
    organization_id,
    'agent',
    CAST(parent_id AS TEXT),
    'agent',
    CAST(id AS TEXT),
    'spawns',
    connected_at
FROM agents
WHERE parent_id IS NOT NULL;
