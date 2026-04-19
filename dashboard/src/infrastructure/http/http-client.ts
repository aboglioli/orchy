import ky, { HTTPError } from 'ky';

import { useAuthStore } from '../../state/auth-store';
import { mapApiError, mapTransportError } from './error';
import type { ApiSuccessEnvelope } from './types';

function isApiSuccessEnvelope<T>(value: unknown): value is ApiSuccessEnvelope<T> {
  return Boolean(value) && typeof value === 'object' && 'data' in (value as Record<string, unknown>);
}

export type HttpClient = {
  get: <T>(path: string) => Promise<T>;
  post: <T>(path: string, body?: unknown) => Promise<T>;
  patch: <T>(path: string, body?: unknown) => Promise<T>;
  put: <T>(path: string, body?: unknown) => Promise<T>;
  delete: <T>(path: string) => Promise<T>;
};

async function request<T>(method: 'get' | 'post' | 'patch' | 'put' | 'delete', path: string, body?: unknown): Promise<T> {
  const { baseUrl, apiKey } = useAuthStore.getState();
  const headers = apiKey ? { Authorization: `Bearer ${apiKey}` } : {};

  try {
    const response = await ky(`${baseUrl}/api${path}`, {
      method,
      headers,
      json: body,
    }).json<unknown>();

    if (isApiSuccessEnvelope<T>(response)) {
      return response.data;
    }

    return response as T;
  } catch (error) {
    if (error instanceof HTTPError) {
      let payload: unknown = undefined;

      try {
        payload = await error.response.clone().json();
      } catch {
        payload = undefined;
      }

      throw mapApiError(payload, error.response.status);
    }

    throw mapTransportError(error);
  }
}

export const httpClient: HttpClient = {
  get(path) {
    return request('get', path);
  },
  post(path, body) {
    return request('post', path, body);
  },
  patch(path, body) {
    return request('patch', path, body);
  },
  put(path, body) {
    return request('put', path, body);
  },
  delete(path) {
    return request('delete', path);
  },
};
