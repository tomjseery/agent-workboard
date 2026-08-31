import { useQuery } from "@tanstack/react-query";

import type { SessionId, WorkspaceId } from "../../../core/generated";
import sessionApi from "../api/sessionApi";
import { sessionQueryKeys } from "../api/sessionQueryKeys";

export function useSessionQuery(workspaceId: WorkspaceId, sessionId: SessionId) {
  return useQuery({ queryKey: sessionQueryKeys.detail(workspaceId, sessionId), queryFn: () => sessionApi.get(workspaceId, sessionId) });
}

export function useRecoveryPreviewQuery(workspaceId: WorkspaceId, sessionId: SessionId) {
  return useQuery({ queryKey: sessionQueryKeys.recovery(workspaceId, sessionId), queryFn: () => sessionApi.recovery(workspaceId, sessionId) });
}
