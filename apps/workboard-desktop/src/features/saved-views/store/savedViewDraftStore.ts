import { create } from "zustand";

import type { BoardViewDefinition, BoardViewDensity, BoardViewGroupingKind, BoardViewId, BoardViewLaneDefinition, BoardViewSortDirection, BoardViewSortField, RepositoryId, WorkItemStatus, WorkspaceId } from "../../../core/contracts";

export interface SavedViewDraft {
  id: BoardViewId;
  workspaceId: WorkspaceId;
  title: string;
  query: string;
  repositoryIds: RepositoryId[];
  groupingKind: BoardViewGroupingKind;
  lanes: BoardViewLaneDefinition[];
  density: BoardViewDensity;
  statuses: WorkItemStatus[];
  sortField: BoardViewSortField;
  sortDirection: BoardViewSortDirection;
  revision: number;
}

interface SavedViewDraftStore {
  draft?: SavedViewDraft;
  begin(workspaceId: WorkspaceId, view?: BoardViewDefinition): void;
  setTitle(title: string): void;
  setQuery(query: string): void;
  toggleRepository(repositoryId: RepositoryId): void;
  setGroupingKind(groupingKind: BoardViewGroupingKind): void;
  setDensity(density: BoardViewDensity): void;
  setSortField(sortField: BoardViewSortField): void;
  setSortDirection(sortDirection: BoardViewSortDirection): void;
  toggleStatus(status: WorkItemStatus): void;
  clear(): void;
}

export const useSavedViewDraftStore = create<SavedViewDraftStore>()((set) => ({
  begin: (workspaceId, view) => set({ draft: view === undefined ? { id: crypto.randomUUID(), workspaceId, title: "", query: "", repositoryIds: [], groupingKind: "hierarchy", lanes: [{ key: "hierarchy", title: "Hierarchy" }], density: "comfortable", statuses: [], sortField: "title", sortDirection: "ascending", revision: 0 } : { id: view.id, workspaceId, title: view.title, query: view.filters.query ?? "", repositoryIds: view.filters.repositoryIds, groupingKind: view.grouping.kind, lanes: view.grouping.lanes, density: view.density, statuses: view.filters.statuses, sortField: view.sort.field, sortDirection: view.sort.direction, revision: view.revision } }),
  setTitle: (title) => set((state) => state.draft === undefined ? state : { draft: { ...state.draft, title } }),
  setQuery: (query) => set((state) => state.draft === undefined ? state : { draft: { ...state.draft, query } }),
  toggleRepository: (repositoryId) => set((state) => state.draft === undefined ? state : { draft: { ...state.draft, repositoryIds: state.draft.repositoryIds.includes(repositoryId) ? state.draft.repositoryIds.filter((id) => id !== repositoryId) : [...state.draft.repositoryIds, repositoryId] } }),
  setGroupingKind: (groupingKind) => set((state) => state.draft === undefined ? state : { draft: { ...state.draft, groupingKind, lanes: [{ key: groupingKind, title: `${groupingKind[0]?.toLocaleUpperCase()}${groupingKind.slice(1)}` }] } }),
  setDensity: (density) => set((state) => state.draft === undefined ? state : { draft: { ...state.draft, density } }),
  setSortField: (sortField) => set((state) => state.draft === undefined ? state : { draft: { ...state.draft, sortField } }),
  setSortDirection: (sortDirection) => set((state) => state.draft === undefined ? state : { draft: { ...state.draft, sortDirection } }),
  toggleStatus: (status) => set((state) => state.draft === undefined ? state : { draft: { ...state.draft, statuses: state.draft.statuses.includes(status) ? state.draft.statuses.filter((value) => value !== status) : [...state.draft.statuses, status] } }),
  clear: () => set({ draft: undefined }),
}));
