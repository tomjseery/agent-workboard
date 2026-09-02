import { useMutation, useQueryClient } from "@tanstack/react-query";

import type { FeatureId, DaemonResponse, WorkspaceId } from "../../../core/contracts";
import { boardQueryKeys } from "../../board/api/boardQueryKeys";
import { hierarchyQueryKeys } from "../../hierarchy/api/hierarchyQueryKeys";
import { proposalQueryKeys } from "../api/proposalQueryKeys";
import proposalApi from "../api/proposalApi";

function useProposalMutation(
  workspaceId: WorkspaceId,
  featureId: FeatureId,
  send: (expectedRevision: number) => Promise<DaemonResponse>,
) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: send,
    onSuccess: (response) => {
      if (response.result?.type === "feature_proposal") {
        queryClient.setQueryData<DaemonResponse>(proposalQueryKeys.detail(workspaceId, featureId), response);
      }
      void queryClient.invalidateQueries({ queryKey: proposalQueryKeys.queue(workspaceId) });
      void queryClient.invalidateQueries({ queryKey: boardQueryKeys.attention(workspaceId) });
      void queryClient.invalidateQueries({ queryKey: hierarchyQueryKeys.workspace(workspaceId) });
    },
  });
}

export function useApproveFeatureMutation(workspaceId: WorkspaceId, featureId: FeatureId) {
  return useProposalMutation(workspaceId, featureId, (expectedRevision) => proposalApi.approve(workspaceId, expectedRevision, featureId));
}

export function useRequestFeatureRevisionMutation(workspaceId: WorkspaceId, featureId: FeatureId) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ expectedRevision, feedback }: { expectedRevision: number; feedback: string }) => proposalApi.requestRevision(workspaceId, expectedRevision, featureId, feedback),
    onSuccess: (response) => {
      if (response.result?.type === "feature_proposal") {
        queryClient.setQueryData<DaemonResponse>(proposalQueryKeys.detail(workspaceId, featureId), response);
      }
      void queryClient.invalidateQueries({ queryKey: proposalQueryKeys.queue(workspaceId) });
      void queryClient.invalidateQueries({ queryKey: boardQueryKeys.attention(workspaceId) });
      void queryClient.invalidateQueries({ queryKey: hierarchyQueryKeys.workspace(workspaceId) });
    },
  });
}
