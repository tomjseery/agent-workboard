import { useMutation, useQueryClient } from "@tanstack/react-query";

import type { DaemonResponse, SessionId, WorkspaceId } from "../../../core/contracts";
import { boardQueryKeys } from "../../board/api/boardQueryKeys";
import { workItemQueryKeys } from "../../work-item/api/workItemQueryKeys";
import sessionApi from "../api/sessionApi";
import { sessionQueryKeys } from "../api/sessionQueryKeys";

export function useRecoverSessionMutation(workspaceId: WorkspaceId, sessionId: SessionId) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (expectedRevision: number) => sessionApi.recover(workspaceId, sessionId, expectedRevision),
    onSuccess: (response: DaemonResponse) => {
      if (response.result?.type === "work_item_detail") {
        queryClient.setQueryData(
          workItemQueryKeys.detail(workspaceId, response.result.value.workItem.id),
          response,
        );
      }
      void queryClient.invalidateQueries({ queryKey: sessionQueryKeys.all(workspaceId) });
      void queryClient.invalidateQueries({ queryKey: sessionQueryKeys.recovery(workspaceId, sessionId) });
      void queryClient.invalidateQueries({ queryKey: boardQueryKeys.boards(workspaceId) });
      void queryClient.invalidateQueries({ queryKey: boardQueryKeys.attention(workspaceId) });
    },
  });
}
