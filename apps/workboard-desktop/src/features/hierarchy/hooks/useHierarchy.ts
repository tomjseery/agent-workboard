import type { BoardViewSort, RepositoryId, WorkItemStatus, WorkspaceHierarchy, WorkspaceId } from "../../../core/generated";
import type { HierarchyEntityKind, HierarchyEntityModel, HierarchyModel } from "../types/hierarchy";
import { useHierarchyQuery } from "./useHierarchyQuery";

function includesQuery(entity: HierarchyEntityModel, query: string) {
  const normalized = query.trim().toLocaleLowerCase();
  return normalized.length === 0 || `${entity.title} ${entity.subtitle}`.toLocaleLowerCase().includes(normalized);
}

export function hierarchyModel(source: WorkspaceHierarchy): HierarchyModel {
  return {
    source,
    repositories: source.repositories.map((repository) => ({ id: repository.id, kind: "repository", title: repository.title, subtitle: repository.slug, repositoryIds: [repository.id] })),
    epics: source.epics.map(({ epic, repositoryIds }) => ({ id: epic.id, kind: "epic", title: epic.title, subtitle: epic.slug, repositoryIds })),
    features: source.features.map(({ feature, repositoryIds }) => ({ id: feature.id, kind: "feature", title: feature.title, subtitle: feature.slug, repositoryIds })),
    workItems: source.workItems.map(({ workItem, repositoryIds, status }) => ({ id: workItem.id, kind: "work_item", title: workItem.title, subtitle: workItem.key, repositoryIds, status })),
  };
}

export function useHierarchy(workspaceId: WorkspaceId, query = "", repositoryIds?: RepositoryId[], statuses?: WorkItemStatus[], sort?: BoardViewSort) {
  const result = useHierarchyQuery(workspaceId);
  const source = result.data?.result?.type === "workspace_hierarchy" ? result.data.result.value : undefined;
  const model = source === undefined ? undefined : hierarchyModel(source);
  const visible = model === undefined
    ? []
    : [...model.repositories, ...model.epics, ...model.features, ...model.workItems]
        .filter(
          (entity) =>
            includesQuery(entity, query) &&
            (repositoryIds === undefined || repositoryIds.length === 0 || entity.repositoryIds.some((id) => repositoryIds.includes(id))) &&
            (statuses === undefined || statuses.length === 0 || (entity.status !== undefined && statuses.includes(entity.status))),
        )
        .sort((left, right) => {
          if (sort === undefined) return 0;
          const leftValue = sort.field === "key" ? left.subtitle : left.title;
          const rightValue = sort.field === "key" ? right.subtitle : right.title;
          return leftValue.localeCompare(rightValue) * (sort.direction === "ascending" ? 1 : -1);
        });
  const find = (kind: HierarchyEntityKind, id: string) => visible.find((entity) => entity.kind === kind && entity.id === id)
    ?? (model === undefined ? undefined : [...model.repositories, ...model.epics, ...model.features, ...model.workItems].find((entity) => entity.kind === kind && entity.id === id));

  return {
    hierarchy: model,
    visible,
    find,
    isLoading: result.isPending,
    isRefreshing: result.isFetching && !result.isPending,
    isUnavailable: result.isError,
  };
}
