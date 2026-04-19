import { create } from 'zustand';
import { createJSONStorage, persist } from 'zustand/middleware';

type AuthState = {
  baseUrl: string;
  apiKey: string;
  setCredentials: (baseUrl: string, apiKey: string) => void;
  clearCredentials: () => void;
};

export const useAuthStore = create<AuthState>()(
  persist(
    (set) => ({
      baseUrl: 'http://localhost:3100',
      apiKey: '',
      setCredentials: (baseUrl, apiKey) => set({ baseUrl, apiKey }),
      clearCredentials: () => set({ baseUrl: 'http://localhost:3100', apiKey: '' }),
    }),
    {
      name: 'orchy-dashboard-auth',
      storage: createJSONStorage(() => sessionStorage),
    },
  ),
);
