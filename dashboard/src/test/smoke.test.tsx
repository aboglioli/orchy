import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { AppProviders } from '../app/providers';

describe('app bootstrap', () => {
  it('renders provider children', () => {
    render(
      <AppProviders>
        <div>dashboard-root</div>
      </AppProviders>,
    );

    expect(screen.getByText('dashboard-root')).toBeInTheDocument();
  });
});
