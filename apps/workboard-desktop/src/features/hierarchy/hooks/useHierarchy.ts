import type { WorkspaceId } from "../../../core/contracts";
import { useHierarchyQuery } from "./useHierarchyQuery";

export function useHierarchy(workspaceId: WorkspaceId) {
  const result = useHierarchyQuery(workspaceId);
  const hierarchy = result.data?.result?.type === "workspace_hierarchy" ? result.data.result.value : undefined;
  return {
    hierarchy,
    isLoading: result.isPending,
    isRefreshing: result.isFetching && !result.isPending,
    isUnavailable: result.isError || (!result.isPending && hierarchy === undefined),
  };
}
