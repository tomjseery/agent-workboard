import { daemon } from "../../../core/daemon";
import type { SessionId, WorkspaceId } from "../../../core/contracts";

const sessionApi = {
  get: (workspaceId: WorkspaceId, sessionId: SessionId) => daemon.sessionObservability(workspaceId, sessionId),
  recovery: (workspaceId: WorkspaceId, sessionId: SessionId) => daemon.recoveryPreview(workspaceId, sessionId),
  recover: (workspaceId: WorkspaceId, sessionId: SessionId, expectedRevision: number) => daemon.execute({
    workspaceId,
    expectedRevision,
    idempotencyKey: crypto.randomUUID(),
    command: { type: "recover_session", value: { sessionId } },
  }),
};

export default sessionApi;
