CREATE INDEX IF NOT EXISTS idx_agents_status_heartbeat ON agents (status, last_heartbeat);
CREATE INDEX IF NOT EXISTS idx_tasks_assigned_to ON tasks (assigned_to);
CREATE INDEX IF NOT EXISTS idx_tasks_status_priority ON tasks (status, priority);
CREATE INDEX IF NOT EXISTS idx_messages_to_target ON messages (to_target);
CREATE INDEX IF NOT EXISTS idx_events_org_timestamp ON events (organization, timestamp);
