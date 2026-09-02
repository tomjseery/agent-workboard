import type { AttentionQuery, BoardQuery, FeatureId, RepositoryId, WorkItemId, WorkItemStatus } from "../../core/contracts";

export interface BoardScope {
  repositoryIds?: RepositoryId[];
  featureIds?: FeatureId[];
}

export interface BoardFilters {
  query: string;
  repositoryIds: RepositoryId[];
  laneKeys: WorkItemStatus[];
  sort: BoardQuery["sort"];
}

export interface BoardInteraction {
  selectedWorkItemId?: WorkItemId;
  focusedWorkItemId?: WorkItemId;
  filters: BoardFilters;
}

export type AttentionFilters = Pick<AttentionQuery, "repositoryIds" | "reasonCodes">;
