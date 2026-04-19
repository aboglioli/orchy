import { httpClient } from '../../infrastructure/http/http-client';
import type { HttpClient } from '../../infrastructure/http/http-client';
import type { User, OrgMembership } from '../../state/auth-store';

export type LoginRequest = {
  email: string;
  password: string;
};

export type RegisterRequest = {
  email: string;
  password: string;
};

export type ChangePasswordRequest = {
  old_password: string;
  new_password: string;
};

export type AuthResponse = {
  user: User;
  memberships: OrgMembership[];
};

export type InviteRequest = {
  email: string;
  role: 'owner' | 'admin' | 'member';
};

export type InviteResponse = {
  user: User;
  membership: OrgMembership;
  is_new_user: boolean;
};

export type AuthService = {
  login: (credentials: LoginRequest) => Promise<AuthResponse>;
  register: (data: RegisterRequest) => Promise<User>;
  logout: () => Promise<void>;
  me: () => Promise<AuthResponse>;
  changePassword: (data: ChangePasswordRequest) => Promise<User>;
  inviteUser: (orgId: string, data: InviteRequest) => Promise<InviteResponse>;
};

export function createAuthService(client: HttpClient): AuthService {
  return {
    login(credentials: LoginRequest): Promise<AuthResponse> {
      return client.post('/auth/login', credentials);
    },

    register(data: RegisterRequest): Promise<User> {
      return client.post('/auth/register', data);
    },

    logout(): Promise<void> {
      return client.post('/auth/logout');
    },

    me(): Promise<AuthResponse> {
      return client.get('/auth/me');
    },

    changePassword(data: ChangePasswordRequest): Promise<User> {
      return client.post('/auth/change-password', data);
    },

    inviteUser(orgId: string, data: InviteRequest): Promise<InviteResponse> {
      return client.post(`/organizations/${encodeURIComponent(orgId)}/invite`, data);
    },
  };
}

export const authService = createAuthService(httpClient);
