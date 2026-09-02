import type { WorkspaceId } from "../../../core/contracts";
import { groupFeaturesByEpic, type FeatureScope } from "../model/overview";
import { useHierarchy } from "./useHierarchy";

export function useFeatureIndex(workspaceId: WorkspaceId, scope: FeatureScope) {
  const { hierarchy, isLoading, isUnavailable } = useHierarchy(workspaceId);
  const epics = hierarchy === undefined ? [] : groupFeaturesByEpic(hierarchy, scope);
  return { epics, isEmpty: epics.length === 0, isLoading, isUnavailable };
}
