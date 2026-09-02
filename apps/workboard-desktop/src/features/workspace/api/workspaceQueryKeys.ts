import type { WorkspaceId } from "../../../core/contracts";

export const workspaceQueryKeys = {
  all: ["workspaces"] as const,
  detail: (workspaceId: WorkspaceId) => ["workspaces", workspaceId] as const,
};
