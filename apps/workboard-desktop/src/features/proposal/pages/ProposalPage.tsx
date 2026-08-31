import type { FeatureId, WorkspaceId } from "../../../core/generated";
import { ProposalDetail } from "../components/ProposalDetail";

export function ProposalPage({ workspaceId, featureId }: { workspaceId: WorkspaceId; featureId: FeatureId }) {
  return <ProposalDetail workspaceId={workspaceId} featureId={featureId} />;
}
