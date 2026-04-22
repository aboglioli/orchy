-- Fix tasks table: add stale_after_secs and last_activity_at columns
-- (Agents table fix moved to 20260422 sync_schema migration which handles both cases)

-- Add stale_after_secs column to tasks
ALTER TABLE tasks ADD COLUMN stale_after_secs INTEGER;

-- Add last_activity_at column to tasks  
ALTER TABLE tasks ADD COLUMN last_activity_at TEXT;

-- Update last_activity_at from updated_at for existing tasks
UPDATE tasks SET last_activity_at = updated_at WHERE last_activity_at IS NULL OR last_activity_at = '1970-01-01T00:00:00+00:00';