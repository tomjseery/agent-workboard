import type { WorkItemId, WorkspaceId } from "../../../core/generated";
import { useWorkItemDetailQuery } from "./useWorkItemQuery";

export function useWorkItemDetail(workspaceId: WorkspaceId, workItemId: WorkItemId) {
  const query = useWorkItemDetailQuery(workspaceId, workItemId);
  return {
    projection: query.data?.result?.type === "work_item_detail" ? query.data.result.value : undefined,
    error: query.data?.error,
    diagnostics: query.data?.diagnostics ?? [],
    partialOutcomes: query.data?.partialOutcomes ?? [],
    isLoading: query.isPending,
    isRefreshing: query.isFetching && !query.isPending,
    isStale: query.isStale,
    isDisconnected: query.isError,
    retry: query.refetch,
  };
}
