import type { WorkspaceId } from "../../../core/generated";

export const hierarchyQueryKeys = {
  all: ["hierarchy"] as const,
  workspace: (workspaceId: WorkspaceId) => ["hierarchy", workspaceId] as const,
};
