import { describe, expect, it } from 'vitest';

import { appRoutes } from './router';

describe('router', () => {
  it('contains multi-org project dashboard route', () => {
    const paths = appRoutes.map((route) => route.path);

    expect(paths).toContain('/orgs/:org/projects/:project/dashboard');
  });
});
