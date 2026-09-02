import type { RepositoryId, WorkspaceId } from "../../../core/contracts";

export const repositoryQueryKeys = {
  all: ["repositories"] as const,
  workspace: (workspaceId: WorkspaceId) => [...repositoryQueryKeys.all, workspaceId] as const,
  detail: (workspaceId: WorkspaceId, repositoryId: RepositoryId) => [...repositoryQueryKeys.workspace(workspaceId), repositoryId] as const,
};
