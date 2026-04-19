import { httpClient } from '../../infrastructure/http/http-client';
import type { HttpClient } from '../../infrastructure/http/http-client';

export type EventDto = {
  topic: string;
  namespace: string;
  payload: unknown;
  timestamp: string;
};

export type PollEventsResponse = {
  since: string;
  count: number;
  events: EventDto[];
};

export type EventService = {
  poll: (org: string, project: string, since?: string, limit?: number) => Promise<PollEventsResponse>;
};

export function createEventService(client: HttpClient): EventService {
  return {
    poll(org: string, project: string, since?: string, limit?: number): Promise<PollEventsResponse> {
      const params = new URLSearchParams();
      if (since) {
        params.set('since', since);
      }
      if (limit !== undefined) {
        params.set('limit', String(limit));
      }
      const query = params.toString();
      const suffix = query ? `?${query}` : '';

      return client.get(
        `/organizations/${encodeURIComponent(org)}/projects/${encodeURIComponent(project)}/events${suffix}`,
      );
    },
  };
}

export const eventService = createEventService(httpClient);
