import { daemon } from "../../../core/daemon";
import type { SessionId, WorkspaceId } from "../../../core/generated";

const sessionApi = {
  get: (workspaceId: WorkspaceId, sessionId: SessionId) => daemon.sessionObservability(workspaceId, sessionId),
  recovery: (workspaceId: WorkspaceId, sessionId: SessionId) => daemon.recoveryPreview(workspaceId, sessionId),
};

export default sessionApi;
