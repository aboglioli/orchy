import type { PropsWithChildren } from 'react';

import { Link } from '@tanstack/react-router';

export function AppLayout({ children }: PropsWithChildren) {
  return (
    <div>
      <header>
        <h1>Orchy admin dashboard</h1>
        <nav>
          <Link to="/orgs">Organizations</Link>
        </nav>
      </header>
      <main>{children}</main>
    </div>
  );
}
