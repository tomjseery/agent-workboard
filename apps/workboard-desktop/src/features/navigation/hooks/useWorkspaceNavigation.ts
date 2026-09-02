import { useParams } from "@tanstack/react-router";

import type { WorkspaceId } from "../../../core/generated";
import { useHierarchy } from "../../hierarchy/hooks/useHierarchy";
import { buildNavigationTree, navigationPath } from "../model/navigationTree";
import { useNavigationStore } from "../store/navigationStore";

export function useWorkspaceNavigation(workspaceId: WorkspaceId) {
  const model = useHierarchy(workspaceId);
  const params = useParams({ strict: false });
  const filter = useNavigationStore((state) => state.filter);
  const overrides = useNavigationStore((state) => state.overrides);
  const setFilter = useNavigationStore((state) => state.setFilter);
  const toggleNode = useNavigationStore((state) => state.toggle);
  const collapseAll = useNavigationStore((state) => state.collapseAll);
  const hierarchy = model.hierarchy?.source;
  const tree = hierarchy === undefined ? undefined : buildNavigationTree(hierarchy, filter);
  const path = hierarchy === undefined ? { repositoryIds: [] } : navigationPath(hierarchy, params);
  const searching = filter.trim().length > 0;
  const openByDefault = (nodeId: string, kind: "repository" | "epic") => {
    if (searching) return true;
    const [scope, entity] = nodeId.split(":");
    if (kind === "repository") return path.repositoryIds.includes(scope) || (scope === "unassigned" && path.repositoryIds.length === 0 && path.epicId !== undefined);
    return entity === path.epicId;
  };

  return {
    tree,
    filter,
    setFilter,
    collapseAll,
    activePath: path,
    isExpanded: (nodeId: string, kind: "repository" | "epic") => overrides[nodeId] ?? openByDefault(nodeId, kind),
    toggle: (nodeId: string, kind: "repository" | "epic") => toggleNode(nodeId, openByDefault(nodeId, kind)),
    isLoading: model.isLoading,
    isUnavailable: model.isUnavailable || model.hierarchy === undefined,
  };
}
