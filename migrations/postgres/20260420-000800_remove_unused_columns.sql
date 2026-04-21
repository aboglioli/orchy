-- Remove unused columns that are not in domain models
-- PostgreSQL supports DROP COLUMN directly

-- Remove notes from tasks (not in Task model)
ALTER TABLE tasks DROP COLUMN IF EXISTS notes;

-- Remove refs from knowledge_entries (not in Knowledge model)
ALTER TABLE knowledge_entries DROP COLUMN IF EXISTS refs;

-- Remove agent_id from knowledge_entries (legacy column)
ALTER TABLE knowledge_entries DROP COLUMN IF EXISTS agent_id;

-- Remove parent_id from agents (legacy column)
ALTER TABLE agents DROP COLUMN IF EXISTS parent_id;

-- Remove parent_id from tasks (legacy column)
ALTER TABLE tasks DROP COLUMN IF EXISTS parent_id;

-- Remove depends_on from tasks (legacy column)
ALTER TABLE tasks DROP COLUMN IF EXISTS depends_on;

-- Drop unused indexes
DROP INDEX IF EXISTS knowledge_entries_agent_idx;
