import type { WorkItemId, WorkspaceId } from "../../../core/generated";
import { HierarchyEntityDetail } from "../components/HierarchyEntityDetail";
import { WorkItemDetail } from "../../work-item/components/WorkItemDetail";

export function WorkItemPage({ workspaceId, workItemId, query, onQueryChange }: { workspaceId: WorkspaceId; workItemId: WorkItemId; query: string; onQueryChange(query: string): void }) {
  return <div className="space-y-6"><HierarchyEntityDetail workspaceId={workspaceId} kind="work_item" entityId={workItemId} query={query} onQueryChange={onQueryChange} /><WorkItemDetail workspaceId={workspaceId} workItemId={workItemId} /></div>;
}
