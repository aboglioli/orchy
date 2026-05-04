import { httpClient } from '../../infrastructure/http/http-client';
import type { HttpClient } from '../../infrastructure/http/http-client';

export type MessageDto = {
  id: string;
  body: string;
  from_agent_id: string;
  created_at: string;
};

export type MessageService = {
  getThread: (_org: string, project: string, messageId: string, limit?: number) => Promise<MessageDto[]>;
};

export function createMessageService(client: HttpClient): MessageService {
  return {
    getThread(_org: string, project: string, messageId: string, limit?: number): Promise<MessageDto[]> {
      const params = new URLSearchParams();
      if (limit !== undefined) {
        params.set('limit', String(limit));
      }
      const query = params.toString();
      const suffix = query ? `?${query}` : '';

      return client.get(
        `/projects/${encodeURIComponent(project)}/messages/${encodeURIComponent(messageId)}/thread${suffix}`,
      );
    },
  };
}

export const messageService = createMessageService(httpClient);
