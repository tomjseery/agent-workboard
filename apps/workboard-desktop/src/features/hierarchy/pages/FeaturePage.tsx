import type { FeatureId, WorkspaceId } from "../../../core/generated";
import { HierarchyEntityDetail } from "../components/HierarchyEntityDetail";

export function FeaturePage({ workspaceId, featureId, query, onQueryChange }: { workspaceId: WorkspaceId; featureId: FeatureId; query: string; onQueryChange(query: string): void }) {
  return <HierarchyEntityDetail workspaceId={workspaceId} kind="feature" entityId={featureId} query={query} onQueryChange={onQueryChange} />;
}
