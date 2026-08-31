import type { WorkspaceId } from "../../../core/generated";
import { ApprovalQueue } from "../components/ApprovalQueue";

export function ApprovalQueuePage({ workspaceId }: { workspaceId: WorkspaceId }) {
  return <ApprovalQueue workspaceId={workspaceId} />;
}
