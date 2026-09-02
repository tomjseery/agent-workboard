import { daemon } from "../../../core/daemon";
import type { Provider, RepositoryId, SessionId, WorkItemId, WorkspaceId } from "../../../core/contracts";

const workItemApi = {
  detail: (workspaceId: WorkspaceId, workItemId: WorkItemId) => daemon.workItemDetail(workspaceId, workItemId),
  startSession: (workspaceId: WorkspaceId, expectedRevision: number, workItemId: WorkItemId, repositoryId: RepositoryId | null, provider: Provider) => daemon.execute({
    workspaceId,
    expectedRevision,
    idempotencyKey: crypto.randomUUID(),
    command: { type: "start_session", value: { workItemId, repositoryId, provider } },
  }),
  resumeSession: (workspaceId: WorkspaceId, expectedRevision: number, sessionId: SessionId) => daemon.execute({
    workspaceId,
    expectedRevision,
    idempotencyKey: crypto.randomUUID(),
    command: { type: "resume_session", value: { sessionId } },
  }),
};

export default workItemApi;
