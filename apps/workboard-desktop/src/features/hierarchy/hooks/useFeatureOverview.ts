import type { FeatureId, WorkspaceId } from "../../../core/contracts";
import { findFeature, participatingRepositories, sortedWorkItems, statusCounts, workItemsForFeature } from "../model/overview";
import { useHierarchy } from "./useHierarchy";

export function useFeatureOverview(workspaceId: WorkspaceId, featureId: FeatureId) {
  const { hierarchy, isLoading, isUnavailable } = useHierarchy(workspaceId);
  if (hierarchy === undefined) return { isLoading, isUnavailable, isMissing: false } as const;

  const entry = findFeature(hierarchy, featureId);
  if (entry === undefined) return { isLoading, isUnavailable, isMissing: true } as const;

  const workItems = workItemsForFeature(hierarchy, featureId);
  return {
    isLoading,
    isUnavailable,
    isMissing: false,
    hierarchy,
    feature: entry.feature,
    workItems: sortedWorkItems(workItems),
    statusCounts: statusCounts(workItems),
    repositories: participatingRepositories(hierarchy, entry.repositoryIds),
  } as const;
}
