import { useQuery } from "@tanstack/react-query";

import workspaceApi from "../api/workspaceApi";
import { workspaceQueryKeys } from "../api/workspaceQueryKeys";
import type { WorkspaceId } from "../../../core/generated";

export function useWorkspaceQuery(workspaceId: WorkspaceId) {
  return useQuery({
    queryKey: workspaceQueryKeys.detail(workspaceId),
    queryFn: () => workspaceApi.get(workspaceId),
  });
}
