-- SQLite doesn't support DROP COLUMN IF EXISTS.
-- These columns were already removed in 20260420-000800 via table recreate.
-- This migration is a no-op for SQLite.
CREATE TABLE IF NOT EXISTS _migration_noop_0600 (id INTEGER);
DROP TABLE IF EXISTS _migration_noop_0600;
