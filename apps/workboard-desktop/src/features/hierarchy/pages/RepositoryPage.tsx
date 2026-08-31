import type { RepositoryId, WorkspaceId } from "../../../core/generated";
import { RepositoryObservabilityPanel } from "../../repository/components/RepositoryObservabilityPanel";
import { HierarchyEntityDetail } from "../components/HierarchyEntityDetail";

export function RepositoryPage({ workspaceId, repositoryId, query, onQueryChange }: { workspaceId: WorkspaceId; repositoryId: RepositoryId; query: string; onQueryChange(query: string): void }) {
  return <div className="space-y-6"><HierarchyEntityDetail workspaceId={workspaceId} kind="repository" entityId={repositoryId} query={query} onQueryChange={onQueryChange} /><RepositoryObservabilityPanel workspaceId={workspaceId} repositoryId={repositoryId} /></div>;
}
