import type { WorkspaceId } from "../../../core/contracts";
import { useHierarchy } from "../../hierarchy/hooks/useHierarchy";
import { repositoryOverviews } from "../../hierarchy/model/overview";
import { useWorkspace } from "./useWorkspace";

export function useWorkspaceOverview(workspaceId: WorkspaceId) {
  const workspace = useWorkspace(workspaceId);
  const { hierarchy, isLoading: isHierarchyLoading } = useHierarchy(workspaceId);
  return {
    summary: workspace.workspace,
    repositories: hierarchy === undefined ? undefined : repositoryOverviews(hierarchy),
    isLoading: workspace.isLoading,
    isRefreshing: workspace.isRefreshing,
    isMissing: workspace.isMissing,
    isRepositoriesLoading: isHierarchyLoading,
  };
}
