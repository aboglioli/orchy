import { httpClient } from '../../infrastructure/http/http-client';
import type { HttpClient } from '../../infrastructure/http/http-client';

export type OrganizationDto = {
  id: string;
  name?: string;
};

export type OrganizationService = {
  listOrganizations: () => Promise<OrganizationDto[]>;
};

export function createOrganizationService(client: HttpClient): OrganizationService {
  return {
    listOrganizations(): Promise<OrganizationDto[]> {
      return client.get('/organizations');
    },
  };
}

export const organizationService = createOrganizationService(httpClient);
