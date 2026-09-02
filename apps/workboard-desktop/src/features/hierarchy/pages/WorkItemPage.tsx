import type { WorkItemId, WorkspaceId } from "../../../core/generated";
import { WorkItemDetail } from "../../work-item/components/WorkItemDetail";
import { useHierarchy } from "../hooks/useHierarchy";
import { Breadcrumbs } from "../components/Breadcrumbs";
import { EntityNotFound } from "../components/EntityNotFound";

export function WorkItemPage({ workspaceId, workItemId }: { workspaceId: WorkspaceId; workItemId: WorkItemId }) {
  const model = useHierarchy(workspaceId);
  if (model.isLoading) return <p role="status">Loading Work item…</p>;
  if (model.isUnavailable || model.hierarchy === undefined) return <p role="alert">The authoritative hierarchy is unavailable.</p>;
  if (model.find("work_item", workItemId) === undefined) return <EntityNotFound kind="work_item" />;
  return <div className="space-y-6"><Breadcrumbs workspaceId={workspaceId} hierarchy={model.hierarchy.source} target={{ kind: "work_item", id: workItemId }} /><WorkItemDetail workspaceId={workspaceId} workItemId={workItemId} /></div>;
}
