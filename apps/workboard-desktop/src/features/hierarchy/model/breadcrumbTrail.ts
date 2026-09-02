import type { EpicId, FeatureId, RepositoryId, WorkItemId, WorkspaceHierarchy } from "../../../core/generated";
import type { HierarchyEntityKind } from "../types/hierarchy";

export type BreadcrumbTarget =
  | { kind: "repository"; id: RepositoryId }
  | { kind: "epic"; id: EpicId }
  | { kind: "feature"; id: FeatureId }
  | { kind: "work_item"; id: WorkItemId };

export interface BreadcrumbStep {
  kind: "workspace" | HierarchyEntityKind;
  id: string;
  title: string;
}

interface Resolved {
  repositoryId?: RepositoryId;
  epicId?: EpicId;
  featureId?: FeatureId;
  workItemId?: WorkItemId;
}

function primaryRepository(hierarchy: WorkspaceHierarchy, repositoryIds: RepositoryId[]) {
  return [...repositoryIds]
    .map((id) => hierarchy.repositories.find((repository) => repository.id === id))
    .filter((repository) => repository !== undefined)
    .sort((left, right) => left.title.localeCompare(right.title))[0]?.id;
}

const resolvers: Record<BreadcrumbTarget["kind"], (hierarchy: WorkspaceHierarchy, id: string) => Resolved> = {
  repository: (_hierarchy, id) => ({ repositoryId: id }),
  epic: (hierarchy, id) => {
    const epic = hierarchy.epics.find((entry) => entry.epic.id === id);
    return { repositoryId: epic === undefined ? undefined : primaryRepository(hierarchy, epic.repositoryIds), epicId: id };
  },
  feature: (hierarchy, id) => {
    const feature = hierarchy.features.find((entry) => entry.feature.id === id);
    return { repositoryId: feature === undefined ? undefined : primaryRepository(hierarchy, feature.repositoryIds), epicId: feature?.feature.epicId, featureId: id };
  },
  work_item: (hierarchy, id) => {
    const workItem = hierarchy.workItems.find((entry) => entry.workItem.id === id);
    const feature = hierarchy.features.find((entry) => entry.feature.id === workItem?.workItem.featureId);
    return {
      repositoryId: workItem === undefined ? undefined : primaryRepository(hierarchy, workItem.repositoryIds),
      epicId: feature?.feature.epicId,
      featureId: workItem?.workItem.featureId,
      workItemId: id,
    };
  },
};

export function breadcrumbTrail(hierarchy: WorkspaceHierarchy, target: BreadcrumbTarget): BreadcrumbStep[] {
  const resolved = resolvers[target.kind](hierarchy, target.id);
  const steps: BreadcrumbStep[] = [{ kind: "workspace", id: hierarchy.workspace.id, title: hierarchy.workspace.title }];
  const repository = hierarchy.repositories.find((entry) => entry.id === resolved.repositoryId);
  if (repository !== undefined) steps.push({ kind: "repository", id: repository.id, title: repository.title });
  const epic = hierarchy.epics.find((entry) => entry.epic.id === resolved.epicId);
  if (epic !== undefined) steps.push({ kind: "epic", id: epic.epic.id, title: epic.epic.title });
  const feature = hierarchy.features.find((entry) => entry.feature.id === resolved.featureId);
  if (feature !== undefined) steps.push({ kind: "feature", id: feature.feature.id, title: feature.feature.title });
  const workItem = hierarchy.workItems.find((entry) => entry.workItem.id === resolved.workItemId);
  if (workItem !== undefined) steps.push({ kind: "work_item", id: workItem.workItem.id, title: `${workItem.workItem.key} ${workItem.workItem.title}` });
  return steps;
}
