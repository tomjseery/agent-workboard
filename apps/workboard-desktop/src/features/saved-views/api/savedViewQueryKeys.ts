import type { BoardViewId, WorkspaceId } from "../../../core/contracts";

export const savedViewQueryKeys = {
  all: ["savedViews"] as const,
  workspace: (workspaceId: WorkspaceId) => ["savedViews", workspaceId] as const,
  list: (workspaceId: WorkspaceId) => ["savedViews", workspaceId, "list"] as const,
  details: (workspaceId: WorkspaceId) => ["savedViews", workspaceId, "detail"] as const,
  detail: (workspaceId: WorkspaceId, viewId: BoardViewId) => ["savedViews", workspaceId, "detail", viewId] as const,
};
