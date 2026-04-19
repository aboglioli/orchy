import type { ApiErrorEnvelope } from './types';

export class ApiClientError extends Error {
  readonly code: string;
  readonly status: number;
  readonly details?: unknown;

  constructor(code: string, message: string, status: number, details?: unknown) {
    super(message);
    this.name = 'ApiClientError';
    this.code = code;
    this.status = status;
    this.details = details;
  }
}

function isApiErrorEnvelope(value: unknown): value is ApiErrorEnvelope {
  if (!value || typeof value !== 'object' || !('error' in value)) {
    return false;
  }

  const error = value.error;

  if (!error || typeof error !== 'object') {
    return false;
  }

  return (
    'code' in error && typeof error.code === 'string' &&
    'message' in error && typeof error.message === 'string'
  );
}

export function mapApiError(value: unknown, status: number): ApiClientError {
  if (isApiErrorEnvelope(value)) {
    return new ApiClientError(value.error.code, value.error.message, status, value.error.details);
  }

  return new ApiClientError('UNEXPECTED_API_RESPONSE', 'Unexpected API response', status, value);
}

export function mapTransportError(error: unknown): ApiClientError {
  if (error instanceof Error) {
    return new ApiClientError('NETWORK_ERROR', error.message, 0, error);
  }

  return new ApiClientError('NETWORK_ERROR', 'Network error', 0, error);
}
