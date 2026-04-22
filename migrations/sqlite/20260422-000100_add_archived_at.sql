-- Add archived_at column for soft-delete/archival support
ALTER TABLE knowledge_entries ADD COLUMN archived_at TEXT;
ALTER TABLE tasks ADD COLUMN archived_at TEXT;
