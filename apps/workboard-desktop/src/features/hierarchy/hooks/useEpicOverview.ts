import type { EpicId, WorkspaceId } from "../../../core/contracts";
import { epicFeatureIds, findEpic, participatingRepositories } from "../model/overview";
import { useHierarchy } from "./useHierarchy";

export function useEpicOverview(workspaceId: WorkspaceId, epicId: EpicId) {
  const { hierarchy, isLoading, isUnavailable } = useHierarchy(workspaceId);
  if (hierarchy === undefined) return { isLoading, isUnavailable, isMissing: false } as const;

  const entry = findEpic(hierarchy, epicId);
  if (entry === undefined) return { isLoading, isUnavailable, isMissing: true } as const;

  return {
    isLoading,
    isUnavailable,
    isMissing: false,
    hierarchy,
    epic: entry.epic,
    featureIds: epicFeatureIds(hierarchy, epicId),
    repositories: participatingRepositories(hierarchy, entry.repositoryIds),
  } as const;
}
