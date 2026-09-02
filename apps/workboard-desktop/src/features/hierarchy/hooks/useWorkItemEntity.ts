import type { WorkItemId, WorkspaceId } from "../../../core/contracts";
import { findWorkItem } from "../model/overview";
import { useHierarchy } from "./useHierarchy";

export function useWorkItemEntity(workspaceId: WorkspaceId, workItemId: WorkItemId) {
  const { hierarchy, isLoading, isUnavailable } = useHierarchy(workspaceId);
  if (hierarchy === undefined) return { isLoading, isUnavailable, isMissing: false } as const;

  const entry = findWorkItem(hierarchy, workItemId);
  if (entry === undefined) return { isLoading, isUnavailable, isMissing: true } as const;

  return { isLoading, isUnavailable, isMissing: false, hierarchy, workItem: entry.workItem } as const;
}
