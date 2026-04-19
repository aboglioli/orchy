import { describe, expect, it } from 'vitest';

import { appRoutes } from '../app/router';
import { parityChecklist } from '../features/parity/parity-checklist';

describe('dashboard parity smoke', () => {
  it('wires all required admin modules to app routes', () => {
    const requiredModules = ['orgs', 'projects', 'tasks', 'knowledge', 'agents', 'messages', 'locks', 'events'];
    const appRoutePaths = appRoutes.map((route) => route.path);

    expect(parityChecklist.map((entry) => entry.group)).toEqual(requiredModules);

    for (const entry of parityChecklist) {
      expect(appRoutePaths).toContain(entry.route);
    }
  });
});
