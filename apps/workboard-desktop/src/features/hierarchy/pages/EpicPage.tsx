import type { EpicId, WorkspaceId } from "../../../core/generated";
import { HierarchyEntityDetail } from "../components/HierarchyEntityDetail";

export function EpicPage({ workspaceId, epicId, query, onQueryChange }: { workspaceId: WorkspaceId; epicId: EpicId; query: string; onQueryChange(query: string): void }) {
  return <HierarchyEntityDetail workspaceId={workspaceId} kind="epic" entityId={epicId} query={query} onQueryChange={onQueryChange} />;
}
