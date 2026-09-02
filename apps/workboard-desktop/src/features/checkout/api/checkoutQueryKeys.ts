import type { CheckoutId, WorkspaceId } from "../../../core/contracts";

export const checkoutQueryKeys = {
  all: ["checkouts"] as const,
  workspace: (workspaceId: WorkspaceId) => [...checkoutQueryKeys.all, workspaceId] as const,
  detail: (workspaceId: WorkspaceId, checkoutId: CheckoutId) => [...checkoutQueryKeys.workspace(workspaceId), checkoutId] as const,
};
