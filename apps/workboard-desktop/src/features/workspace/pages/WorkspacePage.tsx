import type { WorkspaceId } from "../../../core/generated";
import { HierarchyNavigation } from "../../hierarchy/components/HierarchyNavigation";
import { SavedViewsPanel } from "../../saved-views/components/SavedViewsPanel";
import { WorkspaceSummary } from "../components/WorkspaceSummary";
import { useWorkspace } from "../hooks/useWorkspace";

interface WorkspacePageProps {
  workspaceId: WorkspaceId;
  query: string;
  onQueryChange(query: string): void;
}

export function WorkspacePage({ workspaceId, query, onQueryChange }: WorkspacePageProps) {
  const workspace = useWorkspace(workspaceId);
  if (workspace.isLoading) return <p role="status">Loading Workspace…</p>;
  if (workspace.isMissing || workspace.workspace === undefined) return <p role="alert">This Workspace is missing or unavailable.</p>;

  return (
    <div className="space-y-6">
      {workspace.isRefreshing && <p role="status">Refreshing authoritative Workspace data…</p>}
      <WorkspaceSummary summary={workspace.workspace} />
      <SavedViewsPanel workspaceId={workspaceId} />
      <HierarchyNavigation workspaceId={workspaceId} query={query} onQueryChange={onQueryChange} />
    </div>
  );
}
