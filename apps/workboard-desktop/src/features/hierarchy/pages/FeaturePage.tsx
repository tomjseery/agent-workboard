import type { FeatureId, WorkspaceId } from "../../../core/generated";
import { HierarchyEntityDetail } from "../components/HierarchyEntityDetail";
import { ProposalDetail } from "../../proposal/components/ProposalDetail";

export function FeaturePage({ workspaceId, featureId, query, onQueryChange }: { workspaceId: WorkspaceId; featureId: FeatureId; query: string; onQueryChange(query: string): void }) {
  return <div className="space-y-6"><HierarchyEntityDetail workspaceId={workspaceId} kind="feature" entityId={featureId} query={query} onQueryChange={onQueryChange} /><ProposalDetail workspaceId={workspaceId} featureId={featureId} /></div>;
}
