-- Add task staleness columns to match domain model
-- These were omitted from the initial schema

ALTER TABLE tasks ADD COLUMN IF NOT EXISTS stale_after_secs BIGINT;
ALTER TABLE tasks ADD COLUMN IF NOT EXISTS last_activity_at TIMESTAMPTZ NOT NULL DEFAULT NOW();
