import { useQuery } from "@tanstack/react-query";

import type { WorkspaceId } from "../../../core/generated";
import { savedViewQueryKeys } from "../api/savedViewQueryKeys";
import savedViewsApi from "../api/savedViewsApi";

export function useSavedViewsQuery(workspaceId: WorkspaceId) {
  return useQuery({ queryKey: savedViewQueryKeys.list(workspaceId), queryFn: () => savedViewsApi.list(workspaceId) });
}
