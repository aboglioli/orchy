import { describe, expect, it } from 'vitest';

import { mapApiError, mapTransportError } from './error';

describe('mapApiError', () => {
  it('maps envelope error shape', () => {
    const error = mapApiError({ error: { code: 'NOT_FOUND', message: 'missing' } }, 404);

    expect(error.code).toBe('NOT_FOUND');
    expect(error.message).toBe('missing');
    expect(error.status).toBe(404);
  });

  it('maps transport errors as network errors', () => {
    const error = mapTransportError(new Error('timeout'));

    expect(error.code).toBe('NETWORK_ERROR');
    expect(error.message).toBe('timeout');
    expect(error.status).toBe(0);
  });
});
