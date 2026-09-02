import { Link } from "@tanstack/react-router";

import type { WorkspaceId } from "../../../core/generated";
import { useHierarchy } from "../../hierarchy/hooks/useHierarchy";
import { SavedViewsPanel } from "../../saved-views/components/SavedViewsPanel";
import { WorkspaceSummary } from "../components/WorkspaceSummary";
import { useWorkspace } from "../hooks/useWorkspace";

export function WorkspacePage({ workspaceId }: { workspaceId: WorkspaceId }) {
  const workspace = useWorkspace(workspaceId);
  const model = useHierarchy(workspaceId);
  if (workspace.isLoading) return <p role="status">Loading Workspace…</p>;
  if (workspace.isMissing || workspace.workspace === undefined) return <p role="alert">This Workspace is missing or unavailable.</p>;
  const source = model.hierarchy?.source;

  return (
    <div className="space-y-6">
      {workspace.isRefreshing && <p role="status">Refreshing authoritative Workspace data…</p>}
      <WorkspaceSummary summary={workspace.workspace} />
      <section aria-labelledby="workspace-repositories-title" className="rounded-2xl border border-[var(--border)] bg-[var(--surface)] p-5">
        <h2 id="workspace-repositories-title" className="text-lg font-semibold">Repositories</h2>
        {source === undefined ? <p className="mt-2" role="status">Loading repositories…</p> : source.repositories.length === 0 ? <p className="mt-2">No repositories are recorded in this Workspace.</p> : <ul className="mt-4 grid gap-3 sm:grid-cols-2 lg:grid-cols-3">{[...source.repositories].sort((left, right) => left.title.localeCompare(right.title)).map((repository) => { const features = source.features.filter((entry) => entry.repositoryIds.includes(repository.id)).length; const workItems = source.workItems.filter((entry) => entry.repositoryIds.includes(repository.id)).length; return <li key={repository.id}><Link to="/workspaces/$workspaceId/repositories/$repositoryId" params={{ workspaceId, repositoryId: repository.id }} className="block rounded-xl border border-[var(--border)] p-4"><span className="block font-medium">{repository.title}</span><span className="mt-1 block text-sm text-[var(--muted-text)]">{repository.slug}</span><span className="mt-2 block text-xs text-[var(--muted-text)]">{features} Features · {workItems} Work items</span></Link></li>; })}</ul>}
      </section>
      <SavedViewsPanel workspaceId={workspaceId} />
    </div>
  );
}
