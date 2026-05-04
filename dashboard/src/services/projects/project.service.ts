import { httpClient } from '../../infrastructure/http/http-client';
import type { HttpClient } from '../../infrastructure/http/http-client';

export type ProjectDto = {
  id: string;
  description?: string;
  metadata?: Record<string, string>;
};

export type ProjectService = {
  getProject: (_org: string, project: string) => Promise<ProjectDto>;
};

export function createProjectService(client: HttpClient): ProjectService {
  return {
    getProject(_org: string, project: string): Promise<ProjectDto> {
      return client.get(
        `/projects/${encodeURIComponent(project)}`,
      );
    },
  };
}

export const projectService = createProjectService(httpClient);
