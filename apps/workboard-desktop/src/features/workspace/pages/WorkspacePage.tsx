import { Link } from "@tanstack/react-router";

import { Card, CardTitle } from "../../../components/ui/card";
import type { WorkspaceId } from "../../../core/contracts";
import { SavedViewsPanel } from "../../saved-views/components/SavedViewsPanel";
import { WorkspaceSummary } from "../components/WorkspaceSummary";
import { useWorkspaceOverview } from "../hooks/useWorkspaceOverview";

export function WorkspacePage({ workspaceId }: { workspaceId: WorkspaceId }) {
  const overview = useWorkspaceOverview(workspaceId);
  if (overview.isLoading) return <p role="status">Loading Workspace…</p>;
  if (overview.isMissing || overview.summary === undefined) return <p role="alert">This Workspace is missing or unavailable.</p>;

  return (
    <div className="space-y-6">
      {overview.isRefreshing && <p role="status">Refreshing authoritative Workspace data…</p>}
      <WorkspaceSummary summary={overview.summary} />
      <Card asChild>
        <section aria-labelledby="workspace-repositories-title">
          <CardTitle id="workspace-repositories-title">Repositories</CardTitle>
          {overview.repositories === undefined ? (
            <p className="mt-2" role="status">Loading repositories…</p>
          ) : overview.repositories.length === 0 ? (
            <p className="mt-2">No repositories are recorded in this Workspace.</p>
          ) : (
            <ul className="mt-4 grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
              {overview.repositories.map(({ repository, featureCount, workItemCount }) => (
                <li key={repository.id}>
                  <Link
                    to="/workspaces/$workspaceId/repositories/$repositoryId"
                    params={{ workspaceId, repositoryId: repository.id }}
                    search={{ view: "board" }}
                    className="block rounded-xl border border-border p-4"
                  >
                    <span className="block font-medium">{repository.title}</span>
                    <span className="mt-1 block text-sm text-muted-foreground">{repository.slug}</span>
                    <span className="mt-2 block text-xs text-muted-foreground">{featureCount} Features · {workItemCount} Work items</span>
                  </Link>
                </li>
              ))}
            </ul>
          )}
        </section>
      </Card>
      <SavedViewsPanel workspaceId={workspaceId} />
    </div>
  );
}
