import type { SessionId, WorkspaceId } from "../../../core/contracts";
import { useRecoverSessionMutation } from "./useRecoverSessionMutation";
import { useRecoveryPreviewQuery, useSessionQuery } from "./useSessionQuery";

export function useSession(workspaceId: WorkspaceId, sessionId: SessionId) {
  const query = useSessionQuery(workspaceId, sessionId);
  return {
    projection: query.data?.result?.type === "session_observability" ? query.data.result.value : undefined,
    error: query.data?.error,
    isLoading: query.isPending,
    isRefreshing: query.isFetching && !query.isPending,
    isDisconnected: query.isError,
    isPartial: (query.data?.partialOutcomes.length ?? 0) > 0,
    retry: query.refetch,
  };
}

export function useRecovery(workspaceId: WorkspaceId, sessionId: SessionId) {
  const query = useRecoveryPreviewQuery(workspaceId, sessionId);
  const mutation = useRecoverSessionMutation(workspaceId, sessionId);
  return {
    projection: query.data?.result?.type === "recovery_preview" ? query.data.result.value : undefined,
    error: query.data?.error,
    isLoading: query.isPending,
    isRefreshing: query.isFetching && !query.isPending,
    isDisconnected: query.isError,
    isPartial: (query.data?.partialOutcomes.length ?? 0) > 0,
    isRecovering: mutation.isPending,
    outcome: mutation.data,
    recoveryError: mutation.error,
    recover: (expectedRevision: number) => mutation.mutate(expectedRevision),
    retry: query.refetch,
  };
}
