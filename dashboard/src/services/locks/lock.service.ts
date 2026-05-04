import { httpClient } from '../../infrastructure/http/http-client';
import type { HttpClient } from '../../infrastructure/http/http-client';

export type LockDto = {
  name: string;
  holder_agent_id: string;
  expires_at?: string;
};

export type LockService = {
  check: (_org: string, project: string, name: string) => Promise<LockDto | null>;
};

export function createLockService(client: HttpClient): LockService {
  return {
    check(_org: string, project: string, name: string): Promise<LockDto | null> {
      return client.get(
        `/projects/${encodeURIComponent(project)}/locks/${encodeURIComponent(name)}`,
      );
    },
  };
}

export const lockService = createLockService(httpClient);
