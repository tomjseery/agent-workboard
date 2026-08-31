import { useQuery } from "@tanstack/react-query";

import type { FeatureId, WorkspaceId } from "../../../core/generated";
import proposalApi from "../api/proposalApi";
import { proposalQueryKeys } from "../api/proposalQueryKeys";

export function useApprovalQueueQuery(workspaceId: WorkspaceId) {
  return useQuery({ queryKey: proposalQueryKeys.queue(workspaceId), queryFn: () => proposalApi.queue(workspaceId) });
}

export function useFeatureProposalQuery(workspaceId: WorkspaceId, featureId: FeatureId) {
  return useQuery({ queryKey: proposalQueryKeys.detail(workspaceId, featureId), queryFn: () => proposalApi.detail(workspaceId, featureId) });
}
