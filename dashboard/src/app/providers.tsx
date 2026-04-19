import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { PropsWithChildren } from 'react';
import { useEffect, useState } from 'react';

import { authService } from '../services/auth/auth.service';
import { useAuthStore } from '../state/auth-store';

const queryClient = new QueryClient();

function AuthInitializer({ children }: PropsWithChildren) {
  const { setUser, clearAuth, isAuthenticated } = useAuthStore();
  const [isChecking, setIsChecking] = useState(true);

  useEffect(() => {
    // Skip auth check in test environment
    if (import.meta.env.MODE === 'test') {
      setIsChecking(false);
      return;
    }

    async function checkAuth() {
      try {
        const response = await authService.me();
        setUser(response.user, response.memberships);
      } catch {
        clearAuth();
      } finally {
        setIsChecking(false);
      }
    }

    checkAuth();
  }, [setUser, clearAuth]);

  if (isChecking && !isAuthenticated) {
    return <div className="loading">Loading...</div>;
  }

  return children;
}

export function AppProviders({ children }: PropsWithChildren) {
  return (
    <QueryClientProvider client={queryClient}>
      <AuthInitializer>{children}</AuthInitializer>
    </QueryClientProvider>
  );
}
