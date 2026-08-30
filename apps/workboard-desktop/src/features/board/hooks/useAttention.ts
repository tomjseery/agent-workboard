import type { WorkspaceId } from "../../../core/generated";
import { useAttentionQuery } from "./useAttentionQuery";

export function useAttention(workspaceId: WorkspaceId) {
  const query = useAttentionQuery(workspaceId, { limit: 200, repositoryIds: [], reasonCodes: [] });
  const envelopes = query.data?.pages ?? [];
  const projections = envelopes.flatMap((page) => page.result?.type === "attention" ? [page.result.value] : []);
  return {
    entries: projections.flatMap((page) => page.entries),
    totalCount: projections[0]?.totalCount ?? 0,
    error: envelopes.find((page) => page.error !== null)?.error,
    isLoading: query.isPending,
    isRefreshing: query.isFetching && !query.isPending && !query.isFetchingNextPage,
    isPartial: envelopes.some((page) => page.partialOutcomes.length > 0),
    isTransportError: query.isError,
    loadMore: query.fetchNextPage,
    hasMore: query.hasNextPage,
    isLoadingMore: query.isFetchingNextPage,
  };
}
