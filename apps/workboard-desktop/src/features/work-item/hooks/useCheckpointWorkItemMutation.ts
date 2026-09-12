import { useMutation, useQueryClient } from "@tanstack/react-query";

import type { DaemonResponse, WorkItemId, WorkItemStateInput, WorkItemStatus, WorkspaceId } from "../../../core/contracts";
import { boardQueryKeys } from "../../board/api/boardQueryKeys";
import workItemApi from "../api/workItemApi";
import { workItemQueryKeys } from "../api/workItemQueryKeys";
import { createWorkItemStateSchema, type WorkItemStateForm } from "../schemas/workItemStateSchema";

interface CheckpointVariables {
  expectedRevision: number;
  startingStatus: WorkItemStatus;
  state: WorkItemStateForm;
}

export function useCheckpointWorkItemMutation(workspaceId: WorkspaceId, workItemId: WorkItemId) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ expectedRevision, startingStatus, state }: CheckpointVariables) => {
      const input: WorkItemStateInput = createWorkItemStateSchema(startingStatus).parse(state);
      return workItemApi.checkpoint(workspaceId, expectedRevision, workItemId, input);
    },
    onSuccess: (response) => {
      if (response.result?.type === "work_item_detail") {
        queryClient.setQueryData<DaemonResponse>(workItemQueryKeys.detail(workspaceId, workItemId), response);
      }
      void queryClient.invalidateQueries({ queryKey: boardQueryKeys.boards(workspaceId) });
      void queryClient.invalidateQueries({ queryKey: boardQueryKeys.attention(workspaceId) });
    },
  });
}
