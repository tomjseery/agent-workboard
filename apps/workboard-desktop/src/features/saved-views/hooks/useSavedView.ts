import type { BoardViewId, WorkspaceId } from "../../../core/generated";
import { useSavedViewQuery } from "./useSavedViewQuery";

export function useSavedView(workspaceId: WorkspaceId, viewId: BoardViewId) {
  const query = useSavedViewQuery(workspaceId, viewId);
  return {
    view: query.data?.result?.type === "board_view" ? query.data.result.value : undefined,
    isLoading: query.isPending,
    isMissing: query.isError,
  };
}
