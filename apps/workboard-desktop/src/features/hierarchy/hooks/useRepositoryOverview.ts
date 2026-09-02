import type { RepositoryId, WorkspaceId } from "../../../core/contracts";
import { featuresInScope, findRepository, participatingRepositories } from "../model/overview";
import { useHierarchy } from "./useHierarchy";

export function useRepositoryOverview(workspaceId: WorkspaceId, repositoryId: RepositoryId) {
  const { hierarchy, isLoading, isUnavailable } = useHierarchy(workspaceId);
  if (hierarchy === undefined) return { isLoading, isUnavailable, isMissing: false } as const;

  const repository = findRepository(hierarchy, repositoryId);
  if (repository === undefined) return { isLoading, isUnavailable, isMissing: true } as const;

  return {
    isLoading,
    isUnavailable,
    isMissing: false,
    hierarchy,
    repository,
    featureCount: featuresInScope(hierarchy, { repositoryId }).length,
    repositories: participatingRepositories(hierarchy, [repositoryId]),
  } as const;
}
