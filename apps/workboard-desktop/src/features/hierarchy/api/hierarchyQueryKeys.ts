import type { WorkspaceId } from "../../../core/contracts";

export const hierarchyQueryKeys = {
  all: ["hierarchy"] as const,
  workspace: (workspaceId: WorkspaceId) => ["hierarchy", workspaceId] as const,
};
