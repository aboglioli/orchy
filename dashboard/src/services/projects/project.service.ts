import { httpClient } from '../../infrastructure/http/http-client';
import type { HttpClient } from '../../infrastructure/http/http-client';

export type ProjectDto = {
  id: string;
  description?: string;
  metadata?: Record<string, string>;
};

export type ProjectService = {
  getProject: (org: string, project: string) => Promise<ProjectDto>;
};

export function createProjectService(client: HttpClient): ProjectService {
  return {
    getProject(org: string, project: string): Promise<ProjectDto> {
      return client.get(
        `/organizations/${encodeURIComponent(org)}/projects/${encodeURIComponent(project)}`,
      );
    },
  };
}

export const projectService = createProjectService(httpClient);
