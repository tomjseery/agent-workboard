import type { WorkspaceId } from "../../../core/generated";
import { useSavedViewsQuery } from "./useSavedViewsQuery";

export function useSavedViews(workspaceId: WorkspaceId) {
  const query = useSavedViewsQuery(workspaceId);
  const views = query.data?.result?.type === "board_views" ? query.data.result.value : [];
  const saveAction = query.data?.availableActions.find((action) => action.code === "save_board_view");
  return {
    views,
    isLoading: query.isPending,
    isUnavailable: query.isError,
    canSave: saveAction?.available === true,
    readOnlyReason: saveAction?.available === false ? saveAction.unavailableReason?.message : saveAction === undefined ? "This daemon does not advertise saved-view mutation." : undefined,
    workspaceRevision: query.data?.authoritativeRevision ?? 0,
  };
}
