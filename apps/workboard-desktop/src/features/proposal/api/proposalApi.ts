import { daemon } from "../../../core/daemon";
import type { FeatureId, WorkspaceId } from "../../../core/generated";

const proposalApi = {
  queue: (workspaceId: WorkspaceId) => daemon.approvalQueue(workspaceId),
  detail: (workspaceId: WorkspaceId, featureId: FeatureId) => daemon.featureProposal(workspaceId, featureId),
};

export default proposalApi;
