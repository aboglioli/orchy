import type { PropsWithChildren } from 'react';

import { Link, useNavigate } from '@tanstack/react-router';

import { authService } from '../../services/auth/auth.service';
import { useAuthStore } from '../../state/auth-store';

export function AppLayout({ children }: PropsWithChildren) {
  const navigate = useNavigate();
  const { user, isAuthenticated, clearAuth } = useAuthStore();

  async function handleLogout() {
    try {
      await authService.logout();
    } finally {
      clearAuth();
      navigate({ to: '/login' });
    }
  }

  return (
    <div className="app-layout">
      <header>
        <h1>Orchy Admin Dashboard</h1>
        <nav>
          {isAuthenticated ? (
            <>
              <Link to="/orgs">Organizations</Link>
              {user && (
                <span className="user-info">
                  {user.email}
                  {user.is_platform_admin && <span className="badge">Admin</span>}
                </span>
              )}
              <button onClick={handleLogout} className="logout-btn">
                Logout
              </button>
            </>
          ) : (
            <Link to="/login">Login</Link>
          )}
        </nav>
      </header>
      <main>{children}</main>
    </div>
  );
}
