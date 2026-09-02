import { Navigate, createRootRouteWithContext, createRoute, createRouter } from "@tanstack/react-router";

import { AppLayout } from "../app/AppLayout";
import type { WorkspaceId } from "../core/generated";
import { AttentionPage } from "../features/board/pages/AttentionPage";
import { BoardPage } from "../features/board/pages/BoardPage";
import { CheckoutPage } from "../features/checkout/pages/CheckoutPage";
import { EpicPage } from "../features/hierarchy/pages/EpicPage";
import { FeaturePage } from "../features/hierarchy/pages/FeaturePage";
import { RepositoryPage } from "../features/hierarchy/pages/RepositoryPage";
import { WorkItemPage } from "../features/hierarchy/pages/WorkItemPage";
import { ApprovalQueuePage } from "../features/proposal/pages/ApprovalQueuePage";
import { SavedViewPage } from "../features/saved-views/pages/SavedViewPage";
import { SessionPage } from "../features/session/pages/SessionPage";
import { WorkspacePage } from "../features/workspace/pages/WorkspacePage";
import { epicViewSchema, featureTabSchema, repositoryViewSchema } from "./search";

interface RouterContext {
  workspaceId: WorkspaceId;
}

const rootRoute = createRootRouteWithContext<RouterContext>()();

const layoutRoute = createRoute({
  getParentRoute: () => rootRoute,
  id: "_app",
  component: () => {
    const { workspaceId } = layoutRoute.useRouteContext();
    return <AppLayout workspaceId={workspaceId} />;
  },
});

const indexRoute = createRoute({ getParentRoute: () => layoutRoute, path: "/", component: () => { const { workspaceId } = layoutRoute.useRouteContext(); return <Navigate to="/workspaces/$workspaceId" params={{ workspaceId }} />; } });
const workspaceRoute = createRoute({ getParentRoute: () => layoutRoute, path: "/workspaces/$workspaceId", component: () => { const { workspaceId } = workspaceRoute.useParams(); return <WorkspacePage workspaceId={workspaceId} />; } });
const repositoryRoute = createRoute({ getParentRoute: () => layoutRoute, path: "/workspaces/$workspaceId/repositories/$repositoryId", validateSearch: repositoryViewSchema, component: () => { const { workspaceId, repositoryId } = repositoryRoute.useParams(); const { view } = repositoryRoute.useSearch(); const navigate = repositoryRoute.useNavigate(); return <RepositoryPage workspaceId={workspaceId} repositoryId={repositoryId} view={view} onOpenWorkItem={(workItemId) => void navigate({ to: "/workspaces/$workspaceId/work-items/$workItemId", params: { workspaceId, workItemId } })} />; } });
const epicRoute = createRoute({ getParentRoute: () => layoutRoute, path: "/workspaces/$workspaceId/epics/$epicId", validateSearch: epicViewSchema, component: () => { const { workspaceId, epicId } = epicRoute.useParams(); const { view } = epicRoute.useSearch(); const navigate = epicRoute.useNavigate(); return <EpicPage workspaceId={workspaceId} epicId={epicId} view={view} onOpenWorkItem={(workItemId) => void navigate({ to: "/workspaces/$workspaceId/work-items/$workItemId", params: { workspaceId, workItemId } })} />; } });
const featureRoute = createRoute({ getParentRoute: () => layoutRoute, path: "/workspaces/$workspaceId/features/$featureId", validateSearch: featureTabSchema, component: () => { const { workspaceId, featureId } = featureRoute.useParams(); const { tab } = featureRoute.useSearch(); const navigate = featureRoute.useNavigate(); return <FeaturePage workspaceId={workspaceId} featureId={featureId} tab={tab} onOpenWorkItem={(workItemId) => void navigate({ to: "/workspaces/$workspaceId/work-items/$workItemId", params: { workspaceId, workItemId } })} />; } });
const proposalRoute = createRoute({ getParentRoute: () => layoutRoute, path: "/workspaces/$workspaceId/features/$featureId/proposal", component: () => { const { workspaceId, featureId } = proposalRoute.useParams(); const navigate = proposalRoute.useNavigate(); return <FeaturePage workspaceId={workspaceId} featureId={featureId} tab="proposal" onOpenWorkItem={(workItemId) => void navigate({ to: "/workspaces/$workspaceId/work-items/$workItemId", params: { workspaceId, workItemId } })} />; } });
const workItemRoute = createRoute({ getParentRoute: () => layoutRoute, path: "/workspaces/$workspaceId/work-items/$workItemId", component: () => { const { workspaceId, workItemId } = workItemRoute.useParams(); return <WorkItemPage workspaceId={workspaceId} workItemId={workItemId} />; } });
const savedViewRoute = createRoute({ getParentRoute: () => layoutRoute, path: "/workspaces/$workspaceId/views/$viewId", component: () => { const { workspaceId, viewId } = savedViewRoute.useParams(); const navigate = savedViewRoute.useNavigate(); return <SavedViewPage workspaceId={workspaceId} viewId={viewId} onOpenWorkItem={(workItemId) => void navigate({ to: "/workspaces/$workspaceId/work-items/$workItemId", params: { workspaceId, workItemId } })} />; } });
const boardRoute = createRoute({ getParentRoute: () => layoutRoute, path: "/workspaces/$workspaceId/board", component: () => { const { workspaceId } = boardRoute.useParams(); const navigate = boardRoute.useNavigate(); return <BoardPage workspaceId={workspaceId} onOpenWorkItem={(workItemId) => void navigate({ to: "/workspaces/$workspaceId/work-items/$workItemId", params: { workspaceId, workItemId } })} />; } });
const attentionRoute = createRoute({ getParentRoute: () => layoutRoute, path: "/workspaces/$workspaceId/attention", component: () => { const { workspaceId } = attentionRoute.useParams(); const navigate = attentionRoute.useNavigate(); return <AttentionPage workspaceId={workspaceId} onOpenWorkItem={(workItemId) => void navigate({ to: "/workspaces/$workspaceId/work-items/$workItemId", params: { workspaceId, workItemId } })} onOpenFeature={(featureId) => void navigate({ to: "/workspaces/$workspaceId/features/$featureId", params: { workspaceId, featureId }, search: { tab: "proposal" } })} />; } });
const approvalQueueRoute = createRoute({ getParentRoute: () => layoutRoute, path: "/workspaces/$workspaceId/proposals", component: () => { const { workspaceId } = approvalQueueRoute.useParams(); return <ApprovalQueuePage workspaceId={workspaceId} />; } });
const checkoutRoute = createRoute({ getParentRoute: () => layoutRoute, path: "/workspaces/$workspaceId/checkouts/$checkoutId", component: () => { const { workspaceId, checkoutId } = checkoutRoute.useParams(); return <CheckoutPage workspaceId={workspaceId} checkoutId={checkoutId} />; } });
const sessionRoute = createRoute({ getParentRoute: () => layoutRoute, path: "/workspaces/$workspaceId/sessions/$sessionId", component: () => { const { workspaceId, sessionId } = sessionRoute.useParams(); return <SessionPage workspaceId={workspaceId} sessionId={sessionId} />; } });

const routeTree = rootRoute.addChildren([
  layoutRoute.addChildren([indexRoute, workspaceRoute, repositoryRoute, epicRoute, featureRoute, proposalRoute, workItemRoute, savedViewRoute, boardRoute, attentionRoute, approvalQueueRoute, checkoutRoute, sessionRoute]),
]);

export const router = createRouter({ routeTree, context: { workspaceId: "00000000-0000-0000-0000-000000000000" } });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
