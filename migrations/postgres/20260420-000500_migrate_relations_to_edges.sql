-- Phase 2 data migration: relational fields → edges
-- Apply manually after Phase 1 deployment is stable.
-- Idempotent: ON CONFLICT DO NOTHING (edges has no unique constraint, so
-- re-running inserts duplicates — run once only, or add a unique constraint first).

-- Task parent-child → Spawns edges (parent spawned child)
INSERT INTO edges (id, org_id, from_kind, from_id, to_kind, to_id, rel_type, created_at)
SELECT
    gen_random_uuid(),
    organization_id,
    'task',
    parent_id::text,
    'task',
    id::text,
    'spawns',
    created_at
FROM tasks
WHERE parent_id IS NOT NULL;

-- Task dependencies → DependsOn edges
INSERT INTO edges (id, org_id, from_kind, from_id, to_kind, to_id, rel_type, created_at)
SELECT
    gen_random_uuid(),
    t.organization_id,
    'task',
    t.id::text,
    'task',
    dep_id,
    'depends_on',
    t.created_at
FROM tasks t,
     jsonb_array_elements_text(t.depends_on) AS dep_id
WHERE t.depends_on IS NOT NULL
  AND jsonb_typeof(t.depends_on) = 'array'
  AND jsonb_array_length(t.depends_on) > 0;

-- Knowledge authorship → OwnedBy edges
INSERT INTO edges (id, org_id, from_kind, from_id, to_kind, to_id, rel_type, created_at)
SELECT
    gen_random_uuid(),
    organization_id,
    'knowledge',
    id::text,
    'agent',
    agent_id::text,
    'owned_by',
    created_at
FROM knowledge_entries
WHERE agent_id IS NOT NULL;

-- Agent parent hierarchy → Spawns edges (parent spawned child agent)
INSERT INTO edges (id, org_id, from_kind, from_id, to_kind, to_id, rel_type, created_at)
SELECT
    gen_random_uuid(),
    organization_id,
    'agent',
    parent_id::text,
    'agent',
    id::text,
    'spawns',
    connected_at
FROM agents
WHERE parent_id IS NOT NULL;
