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
import { SavedViewPage } from "../features/saved-views/pages/SavedViewPage";
import { SessionPage } from "../features/session/pages/SessionPage";
import { WorkspacePage } from "../features/workspace/pages/WorkspacePage";
import { hierarchySearchSchema } from "./search";

interface RouterContext {
  workspaceId: WorkspaceId;
}

function AppLayout() {
  const { workspaceId } = rootRoute.useRouteContext();
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  useEffect(() => {
    requestAnimationFrame(() => document.getElementById("main-content")?.focus());
  }, [pathname]);
  return <div className="min-h-screen bg-[var(--canvas)] text-[var(--text)]"><header className="border-b border-[var(--border)] bg-[var(--surface)]"><div className="mx-auto flex max-w-[96rem] flex-wrap items-center gap-4 px-5 py-4"><Link to="/workspaces/$workspaceId" params={{ workspaceId }} search={{ q: "" }} className="text-lg font-semibold">Agent Workboard</Link><nav aria-label="Primary" className="flex gap-3 text-sm"><Link to="/workspaces/$workspaceId/board" params={{ workspaceId }} activeProps={{ "aria-current": "page" }}>Board</Link><Link to="/workspaces/$workspaceId/attention" params={{ workspaceId }} activeProps={{ "aria-current": "page" }}>What needs me</Link></nav><span className="ml-auto rounded-full border border-[var(--success-muted)] px-3 py-1 text-xs font-semibold text-[var(--success)]">Daemon connected</span></div></header><main id="main-content" tabIndex={-1} className="mx-auto max-w-[96rem] px-5 py-7"><Outlet /></main></div>;
}

const rootRoute = createRootRouteWithContext<RouterContext>()({ component: AppLayout });
const indexRoute = createRoute({ getParentRoute: () => rootRoute, path: "/", component: () => { const { workspaceId } = rootRoute.useRouteContext(); return <Navigate to="/workspaces/$workspaceId" params={{ workspaceId }} search={{ q: "" }} />; } });
const workspaceRoute = createRoute({ getParentRoute: () => rootRoute, path: "/workspaces/$workspaceId", validateSearch: hierarchySearchSchema, component: () => { const { workspaceId } = workspaceRoute.useParams(); const { q } = workspaceRoute.useSearch(); const navigate = workspaceRoute.useNavigate(); return <WorkspacePage workspaceId={workspaceId} query={q} onQueryChange={(value) => void navigate({ search: { q: value } })} />; } });
const repositoryRoute = createRoute({ getParentRoute: () => rootRoute, path: "/workspaces/$workspaceId/repositories/$repositoryId", validateSearch: hierarchySearchSchema, component: () => { const { workspaceId, repositoryId } = repositoryRoute.useParams(); const { q } = repositoryRoute.useSearch(); const navigate = repositoryRoute.useNavigate(); return <RepositoryPage workspaceId={workspaceId} repositoryId={repositoryId} query={q} onQueryChange={(value) => void navigate({ search: { q: value } })} />; } });
const epicRoute = createRoute({ getParentRoute: () => rootRoute, path: "/workspaces/$workspaceId/epics/$epicId", validateSearch: hierarchySearchSchema, component: () => { const { workspaceId, epicId } = epicRoute.useParams(); const { q } = epicRoute.useSearch(); const navigate = epicRoute.useNavigate(); return <EpicPage workspaceId={workspaceId} epicId={epicId} query={q} onQueryChange={(value) => void navigate({ search: { q: value } })} />; } });
const featureRoute = createRoute({ getParentRoute: () => rootRoute, path: "/workspaces/$workspaceId/features/$featureId", validateSearch: hierarchySearchSchema, component: () => { const { workspaceId, featureId } = featureRoute.useParams(); const { q } = featureRoute.useSearch(); const navigate = featureRoute.useNavigate(); return <FeaturePage workspaceId={workspaceId} featureId={featureId} query={q} onQueryChange={(value) => void navigate({ search: { q: value } })} />; } });
const workItemRoute = createRoute({ getParentRoute: () => rootRoute, path: "/workspaces/$workspaceId/work-items/$workItemId", validateSearch: hierarchySearchSchema, component: () => { const { workspaceId, workItemId } = workItemRoute.useParams(); const { q } = workItemRoute.useSearch(); const navigate = workItemRoute.useNavigate(); return <WorkItemPage workspaceId={workspaceId} workItemId={workItemId} query={q} onQueryChange={(value) => void navigate({ search: { q: value } })} />; } });
const savedViewRoute = createRoute({ getParentRoute: () => rootRoute, path: "/workspaces/$workspaceId/views/$viewId", validateSearch: hierarchySearchSchema, component: () => { const { workspaceId, viewId } = savedViewRoute.useParams(); const { q } = savedViewRoute.useSearch(); const navigate = savedViewRoute.useNavigate(); return <SavedViewPage workspaceId={workspaceId} viewId={viewId} query={q} onQueryChange={(value) => void navigate({ search: { q: value } })} />; } });
const boardRoute = createRoute({ getParentRoute: () => rootRoute, path: "/workspaces/$workspaceId/board", component: () => { const { workspaceId } = boardRoute.useParams(); const navigate = boardRoute.useNavigate(); return <BoardPage workspaceId={workspaceId} onOpenWorkItem={(workItemId) => void navigate({ to: "/workspaces/$workspaceId/work-items/$workItemId", params: { workspaceId, workItemId }, search: { q: "" } })} />; } });
const attentionRoute = createRoute({ getParentRoute: () => rootRoute, path: "/workspaces/$workspaceId/attention", component: () => { const { workspaceId } = attentionRoute.useParams(); const navigate = attentionRoute.useNavigate(); return <AttentionPage workspaceId={workspaceId} onOpenWorkItem={(workItemId) => void navigate({ to: "/workspaces/$workspaceId/work-items/$workItemId", params: { workspaceId, workItemId }, search: { q: "" } })} />; } });
const checkoutRoute = createRoute({ getParentRoute: () => rootRoute, path: "/workspaces/$workspaceId/checkouts/$checkoutId", component: () => { const { workspaceId, checkoutId } = checkoutRoute.useParams(); return <CheckoutPage workspaceId={workspaceId} checkoutId={checkoutId} />; } });
const sessionRoute = createRoute({ getParentRoute: () => rootRoute, path: "/workspaces/$workspaceId/sessions/$sessionId", component: () => { const { workspaceId, sessionId } = sessionRoute.useParams(); return <SessionPage workspaceId={workspaceId} sessionId={sessionId} />; } });

const routeTree = rootRoute.addChildren([indexRoute, workspaceRoute, repositoryRoute, epicRoute, featureRoute, workItemRoute, savedViewRoute, boardRoute, attentionRoute, checkoutRoute, sessionRoute]);
export const router = createRouter({ routeTree, context: { workspaceId: "00000000-0000-0000-0000-000000000000" } });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
