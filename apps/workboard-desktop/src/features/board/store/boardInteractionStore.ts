import { create } from "zustand";

import type { BoardViewSortDirection, BoardViewSortField, RepositoryId, WorkItemId, WorkItemStatus } from "../../../core/generated";
import type { BoardFilters } from "../types/board";

interface BoardInteractionStore {
  selectedWorkItemId?: WorkItemId;
  focusedWorkItemId?: WorkItemId;
  filters: BoardFilters;
  select(workItemId?: WorkItemId): void;
  focus(workItemId?: WorkItemId): void;
  setQuery(query: string): void;
  toggleRepository(repositoryId: RepositoryId): void;
  toggleStatus(status: WorkItemStatus): void;
  setLaneKeys(laneKeys: string[]): void;
  setSort(field: BoardViewSortField, direction: BoardViewSortDirection): void;
  resetFilters(): void;
}

const initialFilters: BoardFilters = { query: "", repositoryIds: [], statuses: [], laneKeys: [], sort: { field: "key", direction: "ascending" } };

export const useBoardInteractionStore = create<BoardInteractionStore>()((set) => ({
  filters: initialFilters,
  select: (selectedWorkItemId) => set({ selectedWorkItemId }),
  focus: (focusedWorkItemId) => set({ focusedWorkItemId }),
  setQuery: (query) => set((state) => ({ filters: { ...state.filters, query } })),
  toggleRepository: (repositoryId) => set((state) => ({ filters: { ...state.filters, repositoryIds: state.filters.repositoryIds.includes(repositoryId) ? state.filters.repositoryIds.filter((id) => id !== repositoryId) : [...state.filters.repositoryIds, repositoryId] } })),
  toggleStatus: (status) => set((state) => ({ filters: { ...state.filters, statuses: state.filters.statuses.includes(status) ? state.filters.statuses.filter((value) => value !== status) : [...state.filters.statuses, status] } })),
  setLaneKeys: (laneKeys) => set((state) => ({ filters: { ...state.filters, laneKeys } })),
  setSort: (field, direction) => set((state) => ({ filters: { ...state.filters, sort: { field, direction } } })),
  resetFilters: () => set({ filters: initialFilters }),
}));
