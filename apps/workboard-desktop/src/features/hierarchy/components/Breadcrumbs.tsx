import { Link } from "@tanstack/react-router";

import type { WorkspaceId } from "../../../core/generated";
import type { HierarchyEntityModel, HierarchyModel } from "../types/hierarchy";
import { EntityLink } from "./EntityLink";

interface BreadcrumbsProps {
  workspaceId: WorkspaceId;
  hierarchy: HierarchyModel;
  entity: HierarchyEntityModel;
}

export function Breadcrumbs({ workspaceId, hierarchy, entity }: BreadcrumbsProps) {
  const feature = entity.kind === "work_item"
    ? hierarchy.features.find((candidate) => hierarchy.source.workItems.find((item) => item.workItem.id === entity.id)?.workItem.featureId === candidate.id)
    : entity.kind === "feature" ? entity : undefined;
  const epic = feature === undefined
    ? entity.kind === "epic" ? entity : undefined
    : hierarchy.epics.find((candidate) => hierarchy.source.features.find((item) => item.feature.id === feature.id)?.feature.epicId === candidate.id);
  const ancestors = [epic, feature, entity].filter((candidate, index, values): candidate is HierarchyEntityModel => candidate !== undefined && values.indexOf(candidate) === index);

  return (
    <nav aria-label="Breadcrumbs">
      <ol className="flex flex-wrap items-center gap-2 text-sm text-[var(--muted-text)]">
        <li><Link to="/workspaces/$workspaceId" params={{ workspaceId }} search={{ q: "" }}>{hierarchy.source.workspace.title}</Link></li>
        {ancestors.map((ancestor) => <li key={`${ancestor.kind}-${ancestor.id}`} className="before:mr-2 before:content-['/']"><EntityLink entity={ancestor} workspaceId={workspaceId} /></li>)}
      </ol>
    </nav>
  );
}
