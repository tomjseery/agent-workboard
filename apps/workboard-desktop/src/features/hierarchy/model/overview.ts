import type {
  EpicId,
  FeatureId,
  HierarchyFeature,
  HierarchyWorkItem,
  RepositoryId,
  RepositoryReference,
  WorkItemId,
  WorkItemStatus,
  WorkspaceHierarchy,
} from "../../../core/contracts";

export interface FeatureScope {
  repositoryId?: RepositoryId;
  epicId?: EpicId;
}

export interface FeatureSummary {
  feature: HierarchyFeature["feature"];
  workItemCount: number;
  statusCounts: StatusCounts;
}

export interface EpicFeatureGroup {
  epicId: EpicId;
  title: string;
  features: FeatureSummary[];
}

export type StatusCounts = Partial<Record<WorkItemStatus, number>>;

const byTitle = (left: { title: string }, right: { title: string }) => left.title.localeCompare(right.title);

export function findRepository(hierarchy: WorkspaceHierarchy, repositoryId: RepositoryId) {
  return hierarchy.repositories.find((entry) => entry.id === repositoryId);
}

export function findEpic(hierarchy: WorkspaceHierarchy, epicId: EpicId) {
  return hierarchy.epics.find((entry) => entry.epic.id === epicId);
}

export function findFeature(hierarchy: WorkspaceHierarchy, featureId: FeatureId) {
  return hierarchy.features.find((entry) => entry.feature.id === featureId);
}

export function findWorkItem(hierarchy: WorkspaceHierarchy, workItemId: WorkItemId) {
  return hierarchy.workItems.find((entry) => entry.workItem.id === workItemId);
}

export function participatingRepositories(hierarchy: WorkspaceHierarchy, repositoryIds: RepositoryId[]): RepositoryReference[] {
  return repositoryIds.flatMap((repositoryId) => {
    const repository = findRepository(hierarchy, repositoryId);
    return repository === undefined ? [] : [repository];
  });
}

export function featuresInScope(hierarchy: WorkspaceHierarchy, scope: FeatureScope): HierarchyFeature[] {
  return hierarchy.features.filter(
    (entry) =>
      (scope.repositoryId === undefined || entry.repositoryIds.includes(scope.repositoryId)) &&
      (scope.epicId === undefined || entry.feature.epicId === scope.epicId),
  );
}

export function workItemsForFeature(hierarchy: WorkspaceHierarchy, featureId: FeatureId, repositoryId?: RepositoryId): HierarchyWorkItem[] {
  return hierarchy.workItems.filter(
    (entry) => entry.workItem.featureId === featureId && (repositoryId === undefined || entry.repositoryIds.includes(repositoryId)),
  );
}

export function statusCounts(workItems: HierarchyWorkItem[]): StatusCounts {
  const counts: StatusCounts = {};
  for (const entry of workItems) counts[entry.status] = (counts[entry.status] ?? 0) + 1;
  return counts;
}

export function groupFeaturesByEpic(hierarchy: WorkspaceHierarchy, scope: FeatureScope): EpicFeatureGroup[] {
  const features = [...featuresInScope(hierarchy, scope)].sort((left, right) => byTitle(left.feature, right.feature));
  const epicIds = [...new Set(features.map((entry) => entry.feature.epicId))];
  return epicIds
    .map((epicId) => ({
      epicId,
      title: findEpic(hierarchy, epicId)?.epic.title ?? "No Epic",
      features: features
        .filter((entry) => entry.feature.epicId === epicId)
        .map((entry) => {
          const workItems = workItemsForFeature(hierarchy, entry.feature.id, scope.repositoryId);
          return { feature: entry.feature, workItemCount: workItems.length, statusCounts: statusCounts(workItems) };
        }),
    }))
    .sort(byTitle);
}

export function repositoryOverviews(hierarchy: WorkspaceHierarchy) {
  return [...hierarchy.repositories].sort(byTitle).map((repository) => ({
    repository,
    featureCount: hierarchy.features.filter((entry) => entry.repositoryIds.includes(repository.id)).length,
    workItemCount: hierarchy.workItems.filter((entry) => entry.repositoryIds.includes(repository.id)).length,
  }));
}

export function epicFeatureIds(hierarchy: WorkspaceHierarchy, epicId: EpicId): FeatureId[] {
  return featuresInScope(hierarchy, { epicId }).map((entry) => entry.feature.id);
}

export function sortedWorkItems(workItems: HierarchyWorkItem[]): HierarchyWorkItem[] {
  return [...workItems].sort((left, right) => left.workItem.key.localeCompare(right.workItem.key));
}
