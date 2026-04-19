import { create } from 'zustand';
import { createJSONStorage, persist } from 'zustand/middleware';

export type User = {
  id: string;
  email: string;
  is_active: boolean;
  is_platform_admin: boolean;
  created_at: string;
};

export type OrgMembership = {
  id: string;
  user_id: string;
  org_id: string;
  role: 'owner' | 'admin' | 'member';
  joined_at: string;
};

type AuthState = {
  baseUrl: string;
  user: User | null;
  memberships: OrgMembership[] | null;
  isAuthenticated: boolean;
  isLoading: boolean;
  setBaseUrl: (baseUrl: string) => void;
  setUser: (user: User, memberships: OrgMembership[]) => void;
  clearAuth: () => void;
};

export const useAuthStore = create<AuthState>()(
  persist(
    (set) => ({
      baseUrl: 'http://localhost:3100',
      user: null,
      memberships: null,
      isAuthenticated: false,
      isLoading: true,
      setBaseUrl: (baseUrl) => set({ baseUrl }),
      setUser: (user, memberships) => set({ user, memberships, isAuthenticated: true, isLoading: false }),
      clearAuth: () => set({ user: null, memberships: null, isAuthenticated: false, isLoading: false }),
    }),
    {
      name: 'orchy-dashboard-auth',
      storage: createJSONStorage(() => sessionStorage),
      partialize: (state) => ({ baseUrl: state.baseUrl }),
    },
  ),
);
