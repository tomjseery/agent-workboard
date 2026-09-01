import { useMutation, useQueryClient } from "@tanstack/react-query";

import type { Provider, RepositoryId, ResponseEnvelope, SessionId, WorkItemId, WorkspaceId } from "../../../core/generated";
import { boardQueryKeys } from "../../board/api/boardQueryKeys";
import { sessionQueryKeys } from "../../session/api/sessionQueryKeys";
import { workItemQueryKeys } from "../api/workItemQueryKeys";
import workItemApi from "../api/workItemApi";

function useDetailMutation<TVariables>(
  workspaceId: WorkspaceId,
  workItemId: WorkItemId,
  send: (variables: TVariables) => Promise<ResponseEnvelope>,
) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: send,
    onSuccess: (response) => {
      if (response.result?.type === "work_item_detail") {
        queryClient.setQueryData<ResponseEnvelope>(workItemQueryKeys.detail(workspaceId, workItemId), response);
      }
      void queryClient.invalidateQueries({ queryKey: sessionQueryKeys.all(workspaceId) });
      void queryClient.invalidateQueries({ queryKey: boardQueryKeys.boards(workspaceId) });
      void queryClient.invalidateQueries({ queryKey: boardQueryKeys.attention(workspaceId) });
    },
  });
}

export interface StartSessionVariables {
  expectedRevision: number;
  repositoryId: RepositoryId | null;
  provider: Provider;
}

export function useStartSessionMutation(workspaceId: WorkspaceId, workItemId: WorkItemId) {
  return useDetailMutation<StartSessionVariables>(workspaceId, workItemId, ({ expectedRevision, repositoryId, provider }) =>
    workItemApi.startSession(workspaceId, expectedRevision, workItemId, repositoryId, provider),
  );
}

export interface ResumeSessionVariables {
  expectedRevision: number;
  sessionId: SessionId;
}

export function useResumeSessionMutation(workspaceId: WorkspaceId, workItemId: WorkItemId) {
  return useDetailMutation<ResumeSessionVariables>(workspaceId, workItemId, ({ expectedRevision, sessionId }) =>
    workItemApi.resumeSession(workspaceId, expectedRevision, sessionId),
  );
}
