import { create } from "zustand";

import type { BoardViewSortDirection, BoardViewSortField, RepositoryId, WorkItemId, WorkItemStatus } from "../../../core/contracts";
import type { BoardFilters } from "../types";
import { defaultLaneKeys, laneOrder } from "../model/presentation";

interface BoardInteractionStore {
  selectedWorkItemId?: WorkItemId;
  focusedWorkItemId?: WorkItemId;
  filters: BoardFilters;
  select(workItemId?: WorkItemId): void;
  focus(workItemId?: WorkItemId): void;
  setQuery(query: string): void;
  toggleRepository(repositoryId: RepositoryId): void;
  toggleLane(laneKey: WorkItemStatus): void;
  setLaneKeys(laneKeys: WorkItemStatus[]): void;
  setSort(field: BoardViewSortField, direction: BoardViewSortDirection): void;
  resetFilters(): void;
}

export const initialBoardFilters: BoardFilters = { query: "", repositoryIds: [], laneKeys: defaultLaneKeys, sort: { field: "key", direction: "ascending" } };

export const useBoardInteractionStore = create<BoardInteractionStore>()((set) => ({
  filters: initialBoardFilters,
  select: (selectedWorkItemId) => set({ selectedWorkItemId }),
  focus: (focusedWorkItemId) => set({ focusedWorkItemId }),
  setQuery: (query) => set((state) => ({ filters: { ...state.filters, query } })),
  toggleRepository: (repositoryId) => set((state) => ({ filters: { ...state.filters, repositoryIds: state.filters.repositoryIds.includes(repositoryId) ? state.filters.repositoryIds.filter((id) => id !== repositoryId) : [...state.filters.repositoryIds, repositoryId] } })),
  toggleLane: (laneKey) => set((state) => ({ filters: { ...state.filters, laneKeys: laneOrder.filter((candidate) => (candidate === laneKey ? !state.filters.laneKeys.includes(candidate) : state.filters.laneKeys.includes(candidate))) } })),
  setLaneKeys: (laneKeys) => set((state) => ({ filters: { ...state.filters, laneKeys } })),
  setSort: (field, direction) => set((state) => ({ filters: { ...state.filters, sort: { field, direction } } })),
  resetFilters: () => set({ filters: initialBoardFilters }),
}));
