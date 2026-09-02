import type { RepositoryId, WorkspaceId } from "../../../core/contracts";
import { useRepositoryQuery } from "./useRepositoryQuery";

export function useRepository(workspaceId: WorkspaceId, repositoryId: RepositoryId) {
  const query = useRepositoryQuery(workspaceId, repositoryId);
  const projection = query.data?.result?.type === "repository_observability" ? query.data.result.value : undefined;
  return {
    projection,
    error: query.data?.error,
    isLoading: query.isPending,
    isRefreshing: query.isFetching && !query.isPending,
    isDisconnected: query.isError,
    isPartial: (query.data?.partialOutcomes.length ?? 0) > 0,
    retry: query.refetch,
  };
}
