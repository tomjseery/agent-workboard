import type { WorkItemId, WorkspaceId } from "../../../core/generated";
import { BoardView } from "../components/BoardView";

interface BoardPageProps {
  workspaceId: WorkspaceId;
  onOpenWorkItem(workItemId: WorkItemId): void;
}

export function BoardPage({ workspaceId, onOpenWorkItem }: BoardPageProps) {
  return <section aria-labelledby="board-title" className="space-y-5"><div><p className="text-sm text-[var(--muted-text)]">Authoritative delivery projection</p><h1 id="board-title" className="text-2xl font-semibold">Board</h1></div><BoardView workspaceId={workspaceId} onOpenWorkItem={onOpenWorkItem} /></section>;
}
