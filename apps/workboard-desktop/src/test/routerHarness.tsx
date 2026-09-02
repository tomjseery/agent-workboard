import { RouterProvider, createMemoryHistory, createRootRoute, createRoute, createRouter } from "@tanstack/react-router";
import type { ReactNode } from "react";

const linkTargets = [
  "/workspaces/$workspaceId",
  "/workspaces/$workspaceId/repositories/$repositoryId",
  "/workspaces/$workspaceId/epics/$epicId",
  "/workspaces/$workspaceId/features/$featureId",
  "/workspaces/$workspaceId/features/$featureId/proposal",
  "/workspaces/$workspaceId/work-items/$workItemId",
  "/workspaces/$workspaceId/views/$viewId",
  "/workspaces/$workspaceId/board",
  "/workspaces/$workspaceId/attention",
  "/workspaces/$workspaceId/proposals",
  "/workspaces/$workspaceId/checkouts/$checkoutId",
  "/workspaces/$workspaceId/sessions/$sessionId",
];

export function RouterHarness({ children }: { children: ReactNode }) {
  const rootRoute = createRootRoute({ component: () => <>{children}</> });
  const routeTree = rootRoute.addChildren([
    createRoute({ getParentRoute: () => rootRoute, path: "/", component: () => null }),
    ...linkTargets.map((path) => createRoute({ getParentRoute: () => rootRoute, path, component: () => null })),
  ]);
  const router = createRouter({ routeTree, history: createMemoryHistory({ initialEntries: ["/"] }) });
  return <RouterProvider router={router as never} />;
}
