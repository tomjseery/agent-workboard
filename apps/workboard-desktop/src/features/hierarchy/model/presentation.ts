import type { HierarchyNode, Owner } from "../../../core/contracts";
import type { HierarchyEntityKind } from "../types";

export const ownerLabels: Record<Owner["kind"], string> = {
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

export interface EntityPresentation {
  eyebrow: string;
  participation: string;
}

export const entityPresentations: Record<HierarchyEntityKind, EntityPresentation> = {
  repository: { eyebrow: "Repository", participation: "Workspace participation" },
  epic: { eyebrow: "Epic", participation: "Participating repositories" },
  feature: { eyebrow: "Feature", participation: "Cross-repository scope" },
  work_item: { eyebrow: "Work item", participation: "Repository scope" },
};
