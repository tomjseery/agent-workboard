import { useMutation, useQueryClient } from "@tanstack/react-query";

import type { DaemonResponse, WorkItemId, WorkItemStateInput, WorkspaceId } from "../../../core/contracts";
import { boardQueryKeys } from "../../board/api/boardQueryKeys";
import workItemApi from "../api/workItemApi";
import { workItemQueryKeys } from "../api/workItemQueryKeys";

interface CheckpointVariables {
  expectedRevision: number;
  state: WorkItemStateInput;
}

export function useCheckpointWorkItemMutation(workspaceId: WorkspaceId, workItemId: WorkItemId) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ expectedRevision, state }: CheckpointVariables) => workItemApi.checkpoint(workspaceId, expectedRevision, workItemId, state),
    onSuccess: (response) => {
      if (response.result?.type === "work_item_detail") {
        queryClient.setQueryData<DaemonResponse>(workItemQueryKeys.detail(workspaceId, workItemId), response);
      }
      void queryClient.invalidateQueries({ queryKey: boardQueryKeys.boards(workspaceId) });
      void queryClient.invalidateQueries({ queryKey: boardQueryKeys.attention(workspaceId) });
    },
  });
}
