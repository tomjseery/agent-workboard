import type { FeatureId, WorkspaceId } from "../../../core/generated";
import { useApprovalQueueQuery, useFeatureProposalQuery } from "./useProposalQuery";

export function useApprovalQueue(workspaceId: WorkspaceId) {
  const query = useApprovalQueueQuery(workspaceId);
  return {
    projection: query.data?.result?.type === "approval_queue" ? query.data.result.value : undefined,
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

export function useFeatureProposal(workspaceId: WorkspaceId, featureId: FeatureId) {
  const query = useFeatureProposalQuery(workspaceId, featureId);
  return {
    projection: query.data?.result?.type === "feature_proposal" ? query.data.result.value : undefined,
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
