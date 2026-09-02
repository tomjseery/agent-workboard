import { Link, Navigate, Outlet, createRootRouteWithContext, createRoute, createRouter, useRouterState } from "@tanstack/react-router";
import { useEffect } from "react";

import type { WorkspaceId } from "../core/generated";
import { AttentionPage } from "../features/board/pages/AttentionPage";
import { BoardPage } from "../features/board/pages/BoardPage";
import { CheckoutPage } from "../features/checkout/pages/CheckoutPage";
import { EpicPage } from "../features/hierarchy/pages/EpicPage";
import { FeaturePage } from "../features/hierarchy/pages/FeaturePage";
import { RepositoryPage } from "../features/hierarchy/pages/RepositoryPage";
import { WorkItemPage } from "../features/hierarchy/pages/WorkItemPage";
import { WorkspaceSidebar } from "../features/navigation/components/WorkspaceSidebar";
import { ApprovalQueuePage } from "../features/proposal/pages/ApprovalQueuePage";
import { SavedViewPage } from "../features/saved-views/pages/SavedViewPage";
import { SessionPage } from "../features/session/pages/SessionPage";
import { WorkspacePage } from "../features/workspace/pages/WorkspacePage";
import { epicViewSchema, featureTabSchema, repositoryViewSchema } from "./search";

interface RouterContext {
  workspaceId: WorkspaceId;
}

const navClasses = "rounded-lg px-3 py-1.5";
const navActiveClasses = "rounded-lg bg-[var(--accent-surface)] px-3 py-1.5 font-semibold text-[var(--accent)]";

function AppLayout() {
  const { workspaceId } = rootRoute.useRouteContext();
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  useEffect(() => {
    requestAnimationFrame(() => document.getElementById("main-content")?.focus());
  }, [pathname]);
  return (
    <div className="min-h-screen bg-[var(--canvas)] text-[var(--text)]">
      <header className="border-b border-[var(--border)] bg-[var(--surface)]">
        <div className="flex flex-wrap items-center gap-4 px-5 py-3">
          <Link to="/workspaces/$workspaceId" params={{ workspaceId }} className="text-lg font-semibold">Agent Workboard</Link>
          <nav aria-label="Primary" className="flex gap-1 text-sm">
            <Link to="/workspaces/$workspaceId/board" params={{ workspaceId }} className={navClasses} activeProps={{ className: navActiveClasses, "aria-current": "page" }}>Board</Link>
            <Link to="/workspaces/$workspaceId/attention" params={{ workspaceId }} className={navClasses} activeProps={{ className: navActiveClasses, "aria-current": "page" }}>What needs me</Link>
            <Link to="/workspaces/$workspaceId/proposals" params={{ workspaceId }} className={navClasses} activeProps={{ className: navActiveClasses, "aria-current": "page" }}>Proposals</Link>
          </nav>
          <span className="ml-auto rounded-full border border-[var(--success-muted)] px-3 py-1 text-xs font-semibold text-[var(--success)]">Daemon connected</span>
        </div>
      </header>
      <div className="flex items-start">
        <WorkspaceSidebar workspaceId={workspaceId} />
        <main id="main-content" tabIndex={-1} className="min-w-0 flex-1 px-5 py-6">
          <Outlet />
        </main>
      </div>
    </div>
  );
}

const rootRoute = createRootRouteWithContext<RouterContext>()({ component: AppLayout });
const indexRoute = createRoute({ getParentRoute: () => rootRoute, path: "/", component: () => { const { workspaceId } = rootRoute.useRouteContext(); return <Navigate to="/workspaces/$workspaceId" params={{ workspaceId }} />; } });
const workspaceRoute = createRoute({ getParentRoute: () => rootRoute, path: "/workspaces/$workspaceId", component: () => { const { workspaceId } = workspaceRoute.useParams(); return <WorkspacePage workspaceId={workspaceId} />; } });
const repositoryRoute = createRoute({ getParentRoute: () => rootRoute, path: "/workspaces/$workspaceId/repositories/$repositoryId", validateSearch: repositoryViewSchema, component: () => { const { workspaceId, repositoryId } = repositoryRoute.useParams(); const { view } = repositoryRoute.useSearch(); const navigate = repositoryRoute.useNavigate(); return <RepositoryPage workspaceId={workspaceId} repositoryId={repositoryId} view={view} onOpenWorkItem={(workItemId) => void navigate({ to: "/workspaces/$workspaceId/work-items/$workItemId", params: { workspaceId, workItemId } })} />; } });
const epicRoute = createRoute({ getParentRoute: () => rootRoute, path: "/workspaces/$workspaceId/epics/$epicId", validateSearch: epicViewSchema, component: () => { const { workspaceId, epicId } = epicRoute.useParams(); const { view } = epicRoute.useSearch(); const navigate = epicRoute.useNavigate(); return <EpicPage workspaceId={workspaceId} epicId={epicId} view={view} onOpenWorkItem={(workItemId) => void navigate({ to: "/workspaces/$workspaceId/work-items/$workItemId", params: { workspaceId, workItemId } })} />; } });
const featureRoute = createRoute({ getParentRoute: () => rootRoute, path: "/workspaces/$workspaceId/features/$featureId", validateSearch: featureTabSchema, component: () => { const { workspaceId, featureId } = featureRoute.useParams(); const { tab } = featureRoute.useSearch(); const navigate = featureRoute.useNavigate(); return <FeaturePage workspaceId={workspaceId} featureId={featureId} tab={tab} onOpenWorkItem={(workItemId) => void navigate({ to: "/workspaces/$workspaceId/work-items/$workItemId", params: { workspaceId, workItemId } })} />; } });
const proposalRoute = createRoute({ getParentRoute: () => rootRoute, path: "/workspaces/$workspaceId/features/$featureId/proposal", component: () => { const { workspaceId, featureId } = proposalRoute.useParams(); const navigate = proposalRoute.useNavigate(); return <FeaturePage workspaceId={workspaceId} featureId={featureId} tab="proposal" onOpenWorkItem={(workItemId) => void navigate({ to: "/workspaces/$workspaceId/work-items/$workItemId", params: { workspaceId, workItemId } })} />; } });
const workItemRoute = createRoute({ getParentRoute: () => rootRoute, path: "/workspaces/$workspaceId/work-items/$workItemId", component: () => { const { workspaceId, workItemId } = workItemRoute.useParams(); return <WorkItemPage workspaceId={workspaceId} workItemId={workItemId} />; } });
const savedViewRoute = createRoute({ getParentRoute: () => rootRoute, path: "/workspaces/$workspaceId/views/$viewId", component: () => { const { workspaceId, viewId } = savedViewRoute.useParams(); const navigate = savedViewRoute.useNavigate(); return <SavedViewPage workspaceId={workspaceId} viewId={viewId} onOpenWorkItem={(workItemId) => void navigate({ to: "/workspaces/$workspaceId/work-items/$workItemId", params: { workspaceId, workItemId } })} />; } });
const boardRoute = createRoute({ getParentRoute: () => rootRoute, path: "/workspaces/$workspaceId/board", component: () => { const { workspaceId } = boardRoute.useParams(); const navigate = boardRoute.useNavigate(); return <BoardPage workspaceId={workspaceId} onOpenWorkItem={(workItemId) => void navigate({ to: "/workspaces/$workspaceId/work-items/$workItemId", params: { workspaceId, workItemId } })} />; } });
const attentionRoute = createRoute({ getParentRoute: () => rootRoute, path: "/workspaces/$workspaceId/attention", component: () => { const { workspaceId } = attentionRoute.useParams(); const navigate = attentionRoute.useNavigate(); return <AttentionPage workspaceId={workspaceId} onOpenWorkItem={(workItemId) => void navigate({ to: "/workspaces/$workspaceId/work-items/$workItemId", params: { workspaceId, workItemId } })} onOpenFeature={(featureId) => void navigate({ to: "/workspaces/$workspaceId/features/$featureId", params: { workspaceId, featureId }, search: { tab: "proposal" } })} />; } });
const approvalQueueRoute = createRoute({ getParentRoute: () => rootRoute, path: "/workspaces/$workspaceId/proposals", component: () => { const { workspaceId } = approvalQueueRoute.useParams(); return <ApprovalQueuePage workspaceId={workspaceId} />; } });
const checkoutRoute = createRoute({ getParentRoute: () => rootRoute, path: "/workspaces/$workspaceId/checkouts/$checkoutId", component: () => { const { workspaceId, checkoutId } = checkoutRoute.useParams(); return <CheckoutPage workspaceId={workspaceId} checkoutId={checkoutId} />; } });
const sessionRoute = createRoute({ getParentRoute: () => rootRoute, path: "/workspaces/$workspaceId/sessions/$sessionId", component: () => { const { workspaceId, sessionId } = sessionRoute.useParams(); return <SessionPage workspaceId={workspaceId} sessionId={sessionId} />; } });

const routeTree = rootRoute.addChildren([indexRoute, workspaceRoute, repositoryRoute, epicRoute, featureRoute, proposalRoute, workItemRoute, savedViewRoute, boardRoute, attentionRoute, approvalQueueRoute, checkoutRoute, sessionRoute]);
export const router = createRouter({ routeTree, context: { workspaceId: "00000000-0000-0000-0000-000000000000" } });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
