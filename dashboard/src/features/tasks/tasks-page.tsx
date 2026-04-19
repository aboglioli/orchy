import { useQuery } from '@tanstack/react-query';

import { taskService } from '../../services/tasks/task.service';

type TasksPageProps = {
  org: string;
  project: string;
};

export function TasksPage({ org, project }: TasksPageProps) {
  const tasksQuery = useQuery({
    queryKey: ['tasks', org, project],
    queryFn: () => taskService.listTasks(org, project),
  });

  if (tasksQuery.isLoading) {
    return <section>Loading tasks...</section>;
  }

  if (tasksQuery.isError) {
    return <section>Failed to load tasks.</section>;
  }

  return (
    <section>
      <h2>Tasks</h2>
      <p>Task list placeholder for {org}/{project}</p>
      <pre>{JSON.stringify(tasksQuery.data, null, 2)}</pre>
    </section>
  );
}
