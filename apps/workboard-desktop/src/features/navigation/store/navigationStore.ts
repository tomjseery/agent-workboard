import { create } from "zustand";

interface NavigationStore {
  filter: string;
  overrides: Record<string, boolean>;
  setFilter(filter: string): void;
  toggle(nodeId: string, openByDefault: boolean): void;
  collapseAll(): void;
}

export const useNavigationStore = create<NavigationStore>()((set) => ({
  filter: "",
  overrides: {},
  setFilter: (filter) => set({ filter }),
  toggle: (nodeId, openByDefault) => set((state) => ({ overrides: { ...state.overrides, [nodeId]: !(state.overrides[nodeId] ?? openByDefault) } })),
  collapseAll: () => set({ overrides: {}, filter: "" }),
}));
