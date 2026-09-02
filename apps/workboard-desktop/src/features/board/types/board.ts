import type { AttentionQuery, BoardQuery, FeatureId, RepositoryId, ResponseEnvelope, ResponseResult, WorkItemId, WorkItemStatus } from "../../../core/generated";

export type BoardResponse = Omit<ResponseEnvelope, "result"> & { result: Extract<ResponseResult, { type: "board" }> | null };
export type AttentionResponse = Omit<ResponseEnvelope, "result"> & { result: Extract<ResponseResult, { type: "attention" }> | null };

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
