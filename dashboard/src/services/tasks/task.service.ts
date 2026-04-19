import { httpClient } from '../../infrastructure/http/http-client';
import type { HttpClient } from '../../infrastructure/http/http-client';

export type GetTaskOptions = {
  includeDependencies?: boolean;
  includeKnowledge?: boolean;
  knowledgeLimit?: number;
  knowledgeKind?: string;
  knowledgeTag?: string;
  knowledgeContentLimit?: number;
};

export type TaskItemDto = {
  id: string;
  title: string;
  status?: string;
};

export type TaskPageDto = {
  items: TaskItemDto[];
  next_cursor?: string | null;
};

export type TaskWithContextDto = {
  task: TaskItemDto & {
    depends_on?: string[];
    acceptance_criteria?: string | null;
  };
  dependencies?: TaskItemDto[];
  knowledge?: Array<{
    id: string;
    title: string;
    kind: string;
    content: string;
  }>;
};

export type TaskService = {
  listTasks: (org: string, project: string) => Promise<TaskPageDto>;
  getTask: (
    org: string,
    project: string,
    id: string,
    options?: GetTaskOptions,
  ) => Promise<TaskWithContextDto>;
};

export function buildGetTaskQuery(options: GetTaskOptions = {}): string {
  const params = new URLSearchParams();

  if (options.includeDependencies) {
    params.set('include_dependencies', 'true');
  }

  if (options.includeKnowledge) {
    params.set('include_knowledge', 'true');
  }

  if (options.knowledgeLimit !== undefined) {
    params.set('knowledge_limit', String(options.knowledgeLimit));
  }

  if (options.knowledgeKind) {
    params.set('knowledge_kind', options.knowledgeKind);
  }

  if (options.knowledgeTag) {
    params.set('knowledge_tag', options.knowledgeTag);
  }

  if (options.knowledgeContentLimit !== undefined) {
    params.set('knowledge_content_limit', String(options.knowledgeContentLimit));
  }

  return params.toString();
}

export function createTaskService(client: HttpClient): TaskService {
  return {
    listTasks(org: string, project: string): Promise<TaskPageDto> {
      const path = `/organizations/${encodeURIComponent(org)}/projects/${encodeURIComponent(project)}/tasks`;

      return client.get(path);
    },

    async getTask(
      org: string,
      project: string,
      id: string,
      options: GetTaskOptions = {},
    ): Promise<TaskWithContextDto> {
      const query = buildGetTaskQuery(options);
      const suffix = query ? `?${query}` : '';
      const path = `/organizations/${encodeURIComponent(org)}/projects/${encodeURIComponent(project)}/tasks/${encodeURIComponent(id)}${suffix}`;

      return client.get(path);
    },
  };
}

export const taskService = createTaskService(httpClient);
