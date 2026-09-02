import { useId, useState } from "react";

import { Alert } from "../../../components/ui/alert";
import { Button } from "../../../components/ui/button";
import { Label } from "../../../components/ui/label";
import { Textarea } from "../../../components/ui/textarea";
import type { AvailableAction, FeatureId, WorkspaceId } from "../../../core/contracts";
import { useApprovalDecision } from "../hooks/useApprovalDecision";

interface ApprovalActionsProps {
  workspaceId: WorkspaceId;
  featureId: FeatureId;
  actions: AvailableAction[];
  revision: number;
}

export function ApprovalActions({ workspaceId, featureId, actions, revision }: ApprovalActionsProps) {
  const feedbackId = useId();
  const [feedback, setFeedback] = useState("");
  const decision = useApprovalDecision(workspaceId, featureId, actions, revision);
  const { approveAction, revisionAction, rejectAction } = decision;

  return (
    <section aria-labelledby="approval-actions-title" className="space-y-4 rounded-xl border border-border p-4">
      <h3 id="approval-actions-title" className="text-lg font-semibold">Approval</h3>
      {decision.isPending && <p role="status">Submitting the approval decision…</p>}
      {decision.responseError != null && (
        <Alert>
          {decision.responseError.message}
          {decision.responseError.currentRevision != null && decision.responseError.staleRevision != null ? " Reload the proposal and try again." : ""}
          <span className="ml-2 text-xs text-muted-foreground">{decision.responseError.code}</span>
        </Alert>
      )}
      {decision.transportError != null && <Alert>The approval decision could not reach Workboard. Retry when the daemon is reachable.</Alert>}
      {decision.outcomes.map((outcome) => (
        <Alert key={outcome.code}>
          {outcome.message}
          {outcome.reconciliationRequired ? " Reconciliation is required." : ""}
        </Alert>
      ))}

      <div className="flex flex-wrap gap-3">
        <Button type="button" disabled={approveAction?.available !== true || decision.isPending} onClick={decision.approve}>
          Approve and publish
        </Button>
        {approveAction?.available !== true && approveAction?.unavailableReason != null && (
          <p className="self-center text-sm text-muted-foreground">{approveAction.unavailableReason.message}</p>
        )}
      </div>

      <div className="space-y-2">
        <Label htmlFor={feedbackId} className="block font-semibold">Revision feedback</Label>
        <Textarea
          id={feedbackId}
          value={feedback}
          onChange={(event) => setFeedback(event.target.value)}
          disabled={revisionAction?.available !== true || decision.isPending}
          rows={4}
          aria-describedby={decision.feedbackError == null ? undefined : `${feedbackId}-error`}
          aria-invalid={decision.feedbackError != null}
        />
        {decision.feedbackError != null && <p id={`${feedbackId}-error`} role="alert" className="text-sm">{decision.feedbackError}</p>}
        <div className="flex flex-wrap gap-3">
          <Button
            type="button"
            disabled={revisionAction?.available !== true || decision.isPending}
            onClick={() => decision.submitRevision(feedback)}
          >
            Request revision
          </Button>
          {revisionAction?.available !== true && revisionAction?.unavailableReason != null && (
            <p className="self-center text-sm text-muted-foreground">{revisionAction.unavailableReason.message}</p>
          )}
        </div>
      </div>

      {rejectAction != null && rejectAction.available !== true && rejectAction.unavailableReason != null && (
        <p className="text-sm text-muted-foreground">
          Reject: {rejectAction.unavailableReason.message}
          <span className="ml-2 text-xs">{rejectAction.unavailableReason.code}</span>
        </p>
      )}
    </section>
  );
}
