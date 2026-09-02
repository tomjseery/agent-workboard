import type { SessionId, WorkspaceId } from "../../../core/contracts";

export const sessionQueryKeys = {
  all: (workspaceId: WorkspaceId) => ["sessions", workspaceId] as const,
  detail: (workspaceId: WorkspaceId, sessionId: SessionId) => [...sessionQueryKeys.all(workspaceId), sessionId] as const,
  recovery: (workspaceId: WorkspaceId, sessionId: SessionId) => ["recovery-previews", workspaceId, sessionId] as const,
};
