import type { WorkItemStatus, WorkspaceHierarchy } from "../../../core/generated";

export type HierarchyEntityKind = "repository" | "epic" | "feature" | "work_item";

export interface HierarchyEntityModel {
  id: string;
  kind: HierarchyEntityKind;
  title: string;
  subtitle: string;
  repositoryIds: string[];
  status?: WorkItemStatus;
}

export interface HierarchyModel {
  source: WorkspaceHierarchy;
  repositories: HierarchyEntityModel[];
  epics: HierarchyEntityModel[];
  features: HierarchyEntityModel[];
  workItems: HierarchyEntityModel[];
}
