import { httpClient } from '../../infrastructure/http/http-client';
import type { HttpClient } from '../../infrastructure/http/http-client';

export type KnowledgeItemDto = {
  id: string;
  path: string;
  title: string;
  kind: string;
};

export type KnowledgePageDto = {
  items: KnowledgeItemDto[];
  next_cursor?: string | null;
};

export type KnowledgeService = {
  listKnowledge: (org: string, project: string) => Promise<KnowledgePageDto>;
};

export function createKnowledgeService(client: HttpClient): KnowledgeService {
  return {
    listKnowledge(org: string, project: string): Promise<KnowledgePageDto> {
      return client.get(
        `/organizations/${encodeURIComponent(org)}/projects/${encodeURIComponent(project)}/knowledge`,
      );
    },
  };
}

export const knowledgeService = createKnowledgeService(httpClient);
