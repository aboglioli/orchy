import { describe, expect, it } from 'vitest';

import { useAuthStore } from './auth-store';

describe('auth store', () => {
  it('sets and clears api key', () => {
    useAuthStore.getState().setCredentials('http://example.local:9999', 'k1');
    expect(useAuthStore.getState().baseUrl).toBe('http://example.local:9999');
    expect(useAuthStore.getState().apiKey).toBe('k1');

    useAuthStore.getState().clearCredentials();

    expect(useAuthStore.getState().baseUrl).toBe('http://localhost:3100');
    expect(useAuthStore.getState().apiKey).toBe('');
  });
});
