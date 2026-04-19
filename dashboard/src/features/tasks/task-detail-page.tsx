import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';

import { taskService } from '../../services/tasks/task.service';
import type { TaskService } from '../../services/tasks/task.service';

type TaskDetailPageProps = {
  org: string;
  project: string;
  taskId: string;
  service?: TaskService;
};

export function TaskDetailPage({
  org,
  project,
  taskId,
  service = taskService,
}: TaskDetailPageProps) {
  const [includeDependencies, setIncludeDependencies] = useState(true);
  const [includeKnowledge, setIncludeKnowledge] = useState(true);
  const [knowledgeLimit, setKnowledgeLimit] = useState(20);

  const taskQuery = useQuery({
    queryKey: [
      'task',
      org,
      project,
      taskId,
      includeDependencies,
      includeKnowledge,
      knowledgeLimit,
    ],
    queryFn: () =>
      service.getTask(org, project, taskId, {
        includeDependencies,
        includeKnowledge,
        knowledgeLimit,
      }),
  });

  if (taskQuery.isLoading) {
    return <section>Loading task...</section>;
  }

  if (taskQuery.isError) {
    return <section>Failed to load task.</section>;
  }

  return (
    <section>
      <h2>Task detail</h2>
      <label>
        <input
          type="checkbox"
          checked={includeDependencies}
          onChange={(event) => setIncludeDependencies(event.target.checked)}
        />
        Include dependencies
      </label>
      <label>
        <input
          type="checkbox"
          checked={includeKnowledge}
          onChange={(event) => setIncludeKnowledge(event.target.checked)}
        />
        Include knowledge
      </label>
      <label htmlFor="knowledge-limit">Knowledge limit</label>
      <input
        id="knowledge-limit"
        type="number"
        min={1}
        value={knowledgeLimit}
        onChange={(event) => setKnowledgeLimit(Number(event.target.value) || 1)}
      />
      <pre>{JSON.stringify(taskQuery.data, null, 2)}</pre>
    </section>
  );
}
