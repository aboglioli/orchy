export type ParityChecklistEntry = {
  group: string;
  route: string;
};

export const parityChecklist: ReadonlyArray<ParityChecklistEntry> = [
  { group: 'orgs', route: '/orgs' },
  { group: 'projects', route: '/orgs/:org/projects/:project/dashboard' },
  { group: 'tasks', route: '/orgs/:org/projects/:project/tasks' },
  { group: 'knowledge', route: '/orgs/:org/projects/:project/knowledge' },
  { group: 'agents', route: '/orgs/:org/projects/:project/agents' },
  { group: 'messages', route: '/orgs/:org/projects/:project/messages' },
  { group: 'locks', route: '/orgs/:org/projects/:project/locks' },
  { group: 'events', route: '/orgs/:org/projects/:project/events' },
];
