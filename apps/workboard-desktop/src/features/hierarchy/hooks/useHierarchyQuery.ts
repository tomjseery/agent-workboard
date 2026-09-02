import { useQuery } from "@tanstack/react-query";

import type { WorkspaceId } from "../../../core/contracts";
import hierarchyApi from "../api/hierarchyApi";
import { hierarchyQueryKeys } from "../api/hierarchyQueryKeys";

export function useHierarchyQuery(workspaceId: WorkspaceId) {
  return useQuery({
    queryKey: hierarchyQueryKeys.workspace(workspaceId),
    queryFn: () => hierarchyApi.get(workspaceId),
  });
}
