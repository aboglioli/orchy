-- Add archived_at column for soft-delete/archival support on knowledge entries
ALTER TABLE knowledge_entries ADD COLUMN IF NOT EXISTS archived_at TIMESTAMPTZ;

-- Add archived_at column for soft-delete/archival support on tasks
ALTER TABLE tasks ADD COLUMN IF NOT EXISTS archived_at TIMESTAMPTZ;

-- Indexes for efficient filtering
CREATE INDEX IF NOT EXISTS idx_knowledge_archived ON knowledge_entries (organization_id, archived_at) WHERE archived_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_tasks_archived ON tasks (organization_id, archived_at) WHERE archived_at IS NOT NULL;