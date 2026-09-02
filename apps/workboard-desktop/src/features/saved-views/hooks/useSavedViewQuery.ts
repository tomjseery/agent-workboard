import { useQuery } from "@tanstack/react-query";

import type { BoardViewId, WorkspaceId } from "../../../core/contracts";
import { savedViewQueryKeys } from "../api/savedViewQueryKeys";
import savedViewsApi from "../api/savedViewsApi";

export function useSavedViewQuery(workspaceId: WorkspaceId, viewId: BoardViewId) {
  return useQuery({ queryKey: savedViewQueryKeys.detail(workspaceId, viewId), queryFn: () => savedViewsApi.get(workspaceId, viewId) });
}
