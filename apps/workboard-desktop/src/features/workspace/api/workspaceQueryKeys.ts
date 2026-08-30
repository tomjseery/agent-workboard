import type { WorkspaceId } from "../../../core/generated";

export const workspaceQueryKeys = {
  all: ["workspaces"] as const,
  detail: (workspaceId: WorkspaceId) => ["workspaces", workspaceId] as const,
};
