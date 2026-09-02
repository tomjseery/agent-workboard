import type { WorkItemId, WorkspaceId } from "../../../core/contracts";
import { WorkItemDetail } from "../../work-item/components/WorkItemDetail";
import { Breadcrumbs } from "../components/Breadcrumbs";
import { EntityNotFound } from "../components/EntityNotFound";
import { useWorkItemEntity } from "../hooks/useWorkItemEntity";

export function WorkItemPage({ workspaceId, workItemId }: { workspaceId: WorkspaceId; workItemId: WorkItemId }) {
  const entity = useWorkItemEntity(workspaceId, workItemId);
  if (entity.isLoading) return <p role="status">Loading Work item…</p>;
  if (entity.isMissing) return <EntityNotFound kind="work_item" />;
  if (entity.isUnavailable || entity.hierarchy === undefined) return <p role="alert">The authoritative hierarchy is unavailable.</p>;

  return (
    <div className="space-y-6">
      <Breadcrumbs workspaceId={workspaceId} hierarchy={entity.hierarchy} target={{ kind: "work_item", id: workItemId }} />
      <WorkItemDetail workspaceId={workspaceId} workItemId={workItemId} />
    </div>
  );
}
