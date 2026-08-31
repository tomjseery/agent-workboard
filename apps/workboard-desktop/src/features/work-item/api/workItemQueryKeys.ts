import type { WorkItemId, WorkspaceId } from "../../../core/generated";

export const workItemQueryKeys = {
  all: ["work-items"] as const,
  workspace: (workspaceId: WorkspaceId) => ["work-items", workspaceId] as const,
  detail: (workspaceId: WorkspaceId, workItemId: WorkItemId) => ["work-items", workspaceId, workItemId] as const,
};
