import { daemon } from "../../../core/daemon";
import type { FeatureId, WorkspaceId } from "../../../core/generated";

const proposalApi = {
  queue: (workspaceId: WorkspaceId) => daemon.approvalQueue(workspaceId),
  detail: (workspaceId: WorkspaceId, featureId: FeatureId) => daemon.featureProposal(workspaceId, featureId),
  approve: (workspaceId: WorkspaceId, expectedRevision: number, featureId: FeatureId) => daemon.execute({
    workspaceId,
    expectedRevision,
    idempotencyKey: crypto.randomUUID(),
    command: { type: "approve_feature", value: { featureId } },
  }),
  requestRevision: (workspaceId: WorkspaceId, expectedRevision: number, featureId: FeatureId, feedback: string) => daemon.execute({
    workspaceId,
    expectedRevision,
    idempotencyKey: crypto.randomUUID(),
    command: { type: "request_feature_revision", value: { featureId, feedback } },
  }),
};

export default proposalApi;
