import type { RepositoryId, WorkspaceId } from "../../../core/generated";

export const repositoryQueryKeys = {
  all: ["repositories"] as const,
  workspace: (workspaceId: WorkspaceId) => [...repositoryQueryKeys.all, workspaceId] as const,
  detail: (workspaceId: WorkspaceId, repositoryId: RepositoryId) => [...repositoryQueryKeys.workspace(workspaceId), repositoryId] as const,
};
