import type { WorkspaceHierarchy, WorkspaceId } from "../../../core/generated";
import type { HierarchyEntityKind, HierarchyEntityModel, HierarchyModel } from "../types/hierarchy";
import { useHierarchyQuery } from "./useHierarchyQuery";

export function hierarchyModel(source: WorkspaceHierarchy): HierarchyModel {
  return {
    source,
    repositories: source.repositories.map((repository) => ({ id: repository.id, kind: "repository", title: repository.title, subtitle: repository.slug, repositoryIds: [repository.id] })),
    epics: source.epics.map(({ epic, repositoryIds }) => ({ id: epic.id, kind: "epic", title: epic.title, subtitle: epic.slug, repositoryIds })),
    features: source.features.map(({ feature, repositoryIds }) => ({ id: feature.id, kind: "feature", title: feature.title, subtitle: feature.slug, repositoryIds })),
    workItems: source.workItems.map(({ workItem, repositoryIds, status }) => ({ id: workItem.id, kind: "work_item", title: workItem.title, subtitle: workItem.key, repositoryIds, status })),
  };
}

export function useHierarchy(workspaceId: WorkspaceId) {
  const result = useHierarchyQuery(workspaceId);
  const source = result.data?.result?.type === "workspace_hierarchy" ? result.data.result.value : undefined;
  const model = source === undefined ? undefined : hierarchyModel(source);
  const find = (kind: HierarchyEntityKind, id: string): HierarchyEntityModel | undefined =>
    model === undefined ? undefined : [...model.repositories, ...model.epics, ...model.features, ...model.workItems].find((entity) => entity.kind === kind && entity.id === id);

  return {
    hierarchy: model,
    find,
    isLoading: result.isPending,
    isRefreshing: result.isFetching && !result.isPending,
    isUnavailable: result.isError,
  };
}
