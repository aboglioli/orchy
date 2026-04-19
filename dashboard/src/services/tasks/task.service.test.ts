import { describe, expect, it } from 'vitest';

import { buildGetTaskQuery } from './task.service';

describe('task service', () => {
  it('builds get task query with expansion flags', () => {
    const query = buildGetTaskQuery({
      includeDependencies: true,
      includeKnowledge: true,
    });

    expect(query).toContain('include_dependencies=true');
    expect(query).toContain('include_knowledge=true');
  });
});
