import type { WorkspaceId } from "../../../core/contracts";
import { useWorkspaceQuery } from "./useWorkspaceQuery";

export function useWorkspace(workspaceId: WorkspaceId) {
  const query = useWorkspaceQuery(workspaceId);
  return {
    workspace: query.data?.result?.type === "workspace_summary" ? query.data.result.value : undefined,
    isLoading: query.isPending,
    isRefreshing: query.isFetching && !query.isPending,
    isMissing: query.isError,
  };
}
