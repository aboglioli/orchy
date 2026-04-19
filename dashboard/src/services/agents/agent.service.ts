import { httpClient } from '../../infrastructure/http/http-client';
import type { HttpClient } from '../../infrastructure/http/http-client';

export type AgentDto = {
  id: string;
  status?: string;
  roles?: string[];
};

export type AgentService = {
  listAgents: (org: string) => Promise<AgentDto[]>;
};

export function createAgentService(client: HttpClient): AgentService {
  return {
    listAgents(org: string): Promise<AgentDto[]> {
      return client.get(`/organizations/${encodeURIComponent(org)}/agents`);
    },
  };
}

export const agentService = createAgentService(httpClient);
