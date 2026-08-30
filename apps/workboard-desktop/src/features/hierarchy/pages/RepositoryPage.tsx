import type { RepositoryId, WorkspaceId } from "../../../core/generated";
import { HierarchyEntityDetail } from "../components/HierarchyEntityDetail";

export function RepositoryPage({ workspaceId, repositoryId, query, onQueryChange }: { workspaceId: WorkspaceId; repositoryId: RepositoryId; query: string; onQueryChange(query: string): void }) {
  return <HierarchyEntityDetail workspaceId={workspaceId} kind="repository" entityId={repositoryId} query={query} onQueryChange={onQueryChange} />;
}
