import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { TaskDetailPage } from './task-detail-page';

describe('task detail query', () => {
  it('calls task service with context expansion flags', async () => {
    const getTask = vi.fn().mockResolvedValue({
      task: { id: 't1', title: 'Task 1' },
      dependencies: [],
      knowledge: [],
    });
    const service = {
      listTasks: vi.fn(),
      getTask,
    };

    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });

    render(
      <QueryClientProvider client={queryClient}>
        <TaskDetailPage org="org1" project="proj1" taskId="task1" service={service} />
      </QueryClientProvider>,
    );

    await waitFor(() => {
      expect(getTask).toHaveBeenCalledWith('org1', 'proj1', 'task1', {
        includeDependencies: true,
        includeKnowledge: true,
        knowledgeLimit: 20,
      });
    });
  });
});
