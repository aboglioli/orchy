import { describe, expect, it } from 'vitest';

import { parityChecklist } from './parity-checklist';

describe('rest parity checklist', () => {
  it('contains tasks, knowledge, agents, messages, locks, events groups', () => {
    const groups = parityChecklist.map((entry) => entry.group);

    expect(groups).toContain('tasks');
    expect(groups).toContain('knowledge');
    expect(groups).toContain('agents');
    expect(groups).toContain('messages');
    expect(groups).toContain('locks');
    expect(groups).toContain('events');
  });
});
