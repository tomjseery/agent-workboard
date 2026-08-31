import { useQuery } from "@tanstack/react-query";

import type { RepositoryId, WorkspaceId } from "../../../core/generated";
import repositoryApi from "../api/repositoryApi";
import { repositoryQueryKeys } from "../api/repositoryQueryKeys";

export function useRepositoryQuery(workspaceId: WorkspaceId, repositoryId: RepositoryId) {
  return useQuery({ queryKey: repositoryQueryKeys.detail(workspaceId, repositoryId), queryFn: () => repositoryApi.get(workspaceId, repositoryId) });
}
