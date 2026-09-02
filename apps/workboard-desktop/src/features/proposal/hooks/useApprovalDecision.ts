import { useState } from "react";

import type { AvailableAction, CommandCode, FeatureId, PartialOutcome, ProtocolError, WorkspaceId } from "../../../core/contracts";
import { requestFeatureRevisionRequestSchema } from "../schemas/requestFeatureRevisionSchema";
import { useApproveFeatureMutation, useRequestFeatureRevisionMutation } from "./useProposalMutations";

function actionFor(actions: AvailableAction[], code: CommandCode) {
  return actions.find((action) => action.code === code);
}

function failureOf(error: unknown): ProtocolError | undefined {
  return typeof error === "object" && error !== null && "code" in error ? (error as ProtocolError) : undefined;
}

export function useApprovalDecision(workspaceId: WorkspaceId, featureId: FeatureId, actions: AvailableAction[], revision: number) {
  const [feedbackError, setFeedbackError] = useState<string | undefined>(undefined);
  const approve = useApproveFeatureMutation(workspaceId, featureId);
  const requestRevision = useRequestFeatureRevisionMutation(workspaceId, featureId);

  const approveAction = actionFor(actions, "approve_feature");
  const revisionAction = actionFor(actions, "request_feature_revision");
  const rejectAction = actionFor(actions, "reject_feature");
  const expectedRevision = approveAction?.expectedRevision ?? revisionAction?.expectedRevision ?? revision;
  const outcomes: PartialOutcome[] = [...(approve.data?.partialOutcomes ?? []), ...(requestRevision.data?.partialOutcomes ?? [])];

  return {
    approveAction,
    revisionAction,
    rejectAction,
    feedbackError,
    outcomes,
    isPending: approve.isPending || requestRevision.isPending,
    responseError: failureOf(approve.data?.error) ?? failureOf(requestRevision.data?.error),
    transportError: approve.error ?? requestRevision.error,
    approve: () => approve.mutate(expectedRevision),
    submitRevision: (feedback: string) => {
      const parsed = requestFeatureRevisionRequestSchema.safeParse({ feedback });
      if (!parsed.success) {
        setFeedbackError(parsed.error.issues[0]?.message);
        return;
      }
      setFeedbackError(undefined);
      requestRevision.mutate({ expectedRevision, feedback: parsed.data.feedback });
    },
  };
}
