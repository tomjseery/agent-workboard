import type { SessionId, WorkspaceId } from "../../../core/contracts";
import { SessionDetail } from "../components/SessionDetail";

export function SessionPage({ workspaceId, sessionId }: { workspaceId: WorkspaceId; sessionId: SessionId }) {
  return <SessionDetail workspaceId={workspaceId} sessionId={sessionId} />;
}
