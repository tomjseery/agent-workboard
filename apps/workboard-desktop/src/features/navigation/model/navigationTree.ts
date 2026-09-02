import type { EpicId, FeatureId, RepositoryId, WorkItemId, WorkspaceHierarchy } from "../../../core/contracts";
import type { NavigationEpicNode, NavigationFeatureNode, NavigationPath, NavigationRepositoryNode, NavigationTree } from "../types";

export const unassignedRepositoryTitle = "No repository participation";
const unassignedEpicTitle = "No Epic";

function matches(needle: string, ...values: string[]) {
  return needle.length === 0 || values.some((value) => value.toLocaleLowerCase().includes(needle));
}

function byTitle<T extends { title: string }>(left: T, right: T) {
  return left.title.localeCompare(right.title);
}

export function buildNavigationTree(hierarchy: WorkspaceHierarchy, filter = ""): NavigationTree {
  const needle = filter.trim().toLocaleLowerCase();
  const epics = new Map(hierarchy.epics.map((entry) => [entry.epic.id, entry.epic]));
  const scopes: Array<{ id: RepositoryId | null; title: string; slug: string }> = [
    ...[...hierarchy.repositories].sort(byTitle).map((repository) => ({ id: repository.id, title: repository.title, slug: repository.slug })),
    { id: null, title: unassignedRepositoryTitle, slug: "" },
  ];
  const claimedEpics = new Set<EpicId>();
  const repositories: NavigationRepositoryNode[] = [];
  let featureCount = 0;

  for (const scope of scopes) {
    const inScope = hierarchy.features.filter((entry) => (scope.id === null ? entry.repositoryIds.length === 0 : entry.repositoryIds.includes(scope.id)));
    const byEpic = new Map<EpicId, NavigationFeatureNode[]>();
    for (const entry of inScope) {
      const items = hierarchy.workItems.filter((item) => item.workItem.featureId === entry.feature.id && (scope.id === null || item.repositoryIds.includes(scope.id)));
      const node: NavigationFeatureNode = {
        id: entry.feature.id,
        nodeId: `${scope.id ?? "unassigned"}:${entry.feature.id}`,
        title: entry.feature.title,
        slug: entry.feature.slug,
        workItemCount: items.length,
        statuses: [...new Set(items.map((item) => item.status))],
      };
      const bucket = byEpic.get(entry.feature.epicId) ?? [];
      bucket.push(node);
      byEpic.set(entry.feature.epicId, bucket);
    }
    if (scope.id === null) for (const [epicId] of epics) if (!claimedEpics.has(epicId) && !byEpic.has(epicId)) byEpic.set(epicId, []);

    const epicNodes: NavigationEpicNode[] = [];
    for (const [epicId, features] of byEpic) {
      const epic = epics.get(epicId);
      claimedEpics.add(epicId);
      const visibleFeatures = features.filter((feature) => matches(needle, feature.title, feature.slug) || matches(needle, epic?.title ?? unassignedEpicTitle, epic?.slug ?? "") || matches(needle, scope.title, scope.slug)).sort(byTitle);
      const epicMatched = matches(needle, epic?.title ?? unassignedEpicTitle, epic?.slug ?? "") || matches(needle, scope.title, scope.slug);
      if (visibleFeatures.length === 0 && !epicMatched) continue;
      epicNodes.push({ id: epicId, nodeId: `${scope.id ?? "unassigned"}:${epicId}`, title: epic?.title ?? unassignedEpicTitle, slug: epic?.slug ?? "", features: visibleFeatures });
      featureCount += visibleFeatures.length;
    }
    epicNodes.sort(byTitle);
    if (epicNodes.length === 0) continue;
    repositories.push({ id: scope.id, nodeId: scope.id ?? "unassigned", title: scope.title, slug: scope.slug, epics: epicNodes });
  }

  return { repositories, featureCount };
}

export function navigationPath(hierarchy: WorkspaceHierarchy, params: { repositoryId?: RepositoryId; epicId?: EpicId; featureId?: FeatureId; workItemId?: WorkItemId }): NavigationPath {
  const workItem = params.workItemId === undefined ? undefined : hierarchy.workItems.find((entry) => entry.workItem.id === params.workItemId);
  const featureId = params.featureId ?? workItem?.workItem.featureId;
  const feature = featureId === undefined ? undefined : hierarchy.features.find((entry) => entry.feature.id === featureId);
  const epicId = params.epicId ?? feature?.feature.epicId;
  const epic = epicId === undefined ? undefined : hierarchy.epics.find((entry) => entry.epic.id === epicId);
  const repositoryIds = params.repositoryId !== undefined ? [params.repositoryId] : workItem?.repositoryIds ?? feature?.repositoryIds ?? epic?.repositoryIds ?? [];
  return { repositoryIds, epicId, featureId };
}
