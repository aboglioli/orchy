import { create } from 'zustand';

type ScopeState = {
  selectedOrg: string;
  selectedProject: string;
  setSelectedOrg: (selectedOrg: string) => void;
  setSelectedProject: (selectedProject: string) => void;
};

export const useScopeStore = create<ScopeState>()((set) => ({
  selectedOrg: '',
  selectedProject: '',
  setSelectedOrg: (selectedOrg) => set({ selectedOrg }),
  setSelectedProject: (selectedProject) => set({ selectedProject }),
}));
