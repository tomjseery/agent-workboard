import type { WorkItemId, WorkspaceId } from "../../../core/generated";
import { HierarchyEntityDetail } from "../components/HierarchyEntityDetail";

export function WorkItemPage({ workspaceId, workItemId, query, onQueryChange }: { workspaceId: WorkspaceId; workItemId: WorkItemId; query: string; onQueryChange(query: string): void }) {
  return <HierarchyEntityDetail workspaceId={workspaceId} kind="work_item" entityId={workItemId} query={query} onQueryChange={onQueryChange} />;
}
