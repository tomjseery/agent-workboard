import type { FeatureId, WorkspaceId } from "../../../core/contracts";

export const proposalQueryKeys = {
  all: (workspaceId: WorkspaceId) => ["proposals", workspaceId] as const,
  queue: (workspaceId: WorkspaceId) => [...proposalQueryKeys.all(workspaceId), "approval-queue"] as const,
  details: (workspaceId: WorkspaceId) => [...proposalQueryKeys.all(workspaceId), "detail"] as const,
  detail: (workspaceId: WorkspaceId, featureId: FeatureId) => [...proposalQueryKeys.details(workspaceId), featureId] as const,
};
