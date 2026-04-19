import { describe, expect, it } from 'vitest';

import { useAuthStore } from './auth-store';

describe('auth store', () => {
  it('sets and clears user', () => {
    const mockUser = {
      id: 'test-user-id',
      email: 'test@example.com',
      is_active: true,
      is_platform_admin: false,
      created_at: '2024-01-01T00:00:00Z',
    };

    const mockMemberships = [
      {
        id: 'test-membership-id',
        user_id: 'test-user-id',
        org_id: 'test-org',
        role: 'member' as const,
        joined_at: '2024-01-01T00:00:00Z',
      },
    ];

    useAuthStore.getState().setUser(mockUser, mockMemberships);
    expect(useAuthStore.getState().user).toEqual(mockUser);
    expect(useAuthStore.getState().memberships).toEqual(mockMemberships);
    expect(useAuthStore.getState().isAuthenticated).toBe(true);

    useAuthStore.getState().clearAuth();

    expect(useAuthStore.getState().user).toBeNull();
    expect(useAuthStore.getState().memberships).toBeNull();
    expect(useAuthStore.getState().isAuthenticated).toBe(false);
  });

  it('sets base URL', () => {
    useAuthStore.getState().setBaseUrl('http://example.local:9999');
    expect(useAuthStore.getState().baseUrl).toBe('http://example.local:9999');
  });
});
