import { useQuery } from "@tanstack/react-query";

import type { WorkItemId, WorkspaceId } from "../../../core/generated";
import workItemApi from "../api/workItemApi";
import { workItemQueryKeys } from "../api/workItemQueryKeys";

export function useWorkItemDetailQuery(workspaceId: WorkspaceId, workItemId: WorkItemId) {
  return useQuery({ queryKey: workItemQueryKeys.detail(workspaceId, workItemId), queryFn: () => workItemApi.detail(workspaceId, workItemId) });
}
