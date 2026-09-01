import type { HierarchyNode, OwnerProjection } from "../../../core/generated";

export const ownerLabels: Record<OwnerProjection["kind"], string> = {
  workspace: "Workspace",
  epic: "Epic",
  feature: "Feature",
  work_item: "Work item",
};

export const hierarchyNodeLabels: Record<HierarchyNode["kind"], string> = {
  repository: "Repository",
  epic: "Epic",
  feature: "Feature",
  work_item: "Work item",
};
