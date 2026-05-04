CREATE INDEX IF NOT EXISTS idx_tasks_org_project_status_assigned
    ON tasks (organization_id, project, status, assigned_to);

CREATE INDEX IF NOT EXISTS idx_knowledge_org_project_namespace_kind
    ON knowledge_entries (organization_id, project, namespace, kind);
