import { useId, useState } from "react";

import type { AvailableAction, CommandCode, FeatureId, PartialOutcome, ProtocolError, WorkspaceId } from "../../../core/generated";
import { useApproveFeatureMutation, useRequestFeatureRevisionMutation } from "../hooks/useProposalMutations";

interface ApprovalActionsProps {
  workspaceId: WorkspaceId;
  featureId: FeatureId;
  actions: AvailableAction[];
  revision: number;
}

function actionFor(actions: AvailableAction[], code: CommandCode) {
  return actions.find((action) => action.code === code);
}

function failureOf(error: unknown): ProtocolError | undefined {
  return typeof error === "object" && error !== null && "code" in error ? (error as ProtocolError) : undefined;
}

export function ApprovalActions({ workspaceId, featureId, actions, revision }: ApprovalActionsProps) {
  const feedbackId = useId();
  const [feedback, setFeedback] = useState("");
  const [feedbackError, setFeedbackError] = useState<string | undefined>(undefined);
  const approve = useApproveFeatureMutation(workspaceId, featureId);
  const requestRevision = useRequestFeatureRevisionMutation(workspaceId, featureId);

  const approveAction = actionFor(actions, "approve_feature");
  const revisionAction = actionFor(actions, "request_feature_revision");
  const rejectAction = actionFor(actions, "reject_feature");
  const busy = approve.isPending || requestRevision.isPending;
  const expectedRevision = approveAction?.expectedRevision ?? revisionAction?.expectedRevision ?? revision;

  const responseError = failureOf(approve.data?.error) ?? failureOf(requestRevision.data?.error);
  const transportError = approve.error ?? requestRevision.error;
  const outcomes: PartialOutcome[] = [...(approve.data?.partialOutcomes ?? []), ...(requestRevision.data?.partialOutcomes ?? [])];

  function submitRevision() {
    const trimmed = feedback.trim();
    if (trimmed.length === 0) {
      setFeedbackError("Describe what the planner must change before requesting a revision.");
      return;
    }
    setFeedbackError(undefined);
    requestRevision.mutate({ expectedRevision, feedback: trimmed });
  }

  return (
    <section aria-labelledby="approval-actions-title" className="space-y-4 rounded-xl border border-[var(--border)] p-4">
      <h3 id="approval-actions-title" className="text-lg font-semibold">Approval</h3>
      {busy && <p role="status">Submitting the approval decision…</p>}
      {responseError != null && (
        <p role="alert" className="rounded-lg border border-[var(--warning-muted)] p-3">
          {responseError.message}
          {responseError.currentRevision != null && responseError.staleRevision != null ? " Reload the proposal and try again." : ""}
          <span className="ml-2 text-xs text-[var(--muted-text)]">{responseError.code}</span>
        </p>
      )}
      {transportError != null && <p role="alert" className="rounded-lg border border-[var(--warning-muted)] p-3">The approval decision could not reach Workboard. Retry when the daemon is reachable.</p>}
      {outcomes.map((outcome) => (
        <p key={outcome.code} role="alert" className="rounded-lg border border-[var(--warning-muted)] p-3">
          {outcome.message}
          {outcome.reconciliationRequired ? " Reconciliation is required." : ""}
        </p>
      ))}

      <div className="flex flex-wrap gap-3">
        <button
          type="button"
          disabled={approveAction?.available !== true || busy}
          onClick={() => approve.mutate(expectedRevision)}
          className="rounded-lg border border-[var(--border)] px-3 py-2 disabled:opacity-50"
        >
          Approve and publish
        </button>
        {approveAction?.available !== true && approveAction?.unavailableReason != null && (
          <p className="self-center text-sm text-[var(--muted-text)]">{approveAction.unavailableReason.message}</p>
        )}
      </div>

      <div className="space-y-2">
        <label htmlFor={feedbackId} className="block text-sm font-semibold">Revision feedback</label>
        <textarea
          id={feedbackId}
          value={feedback}
          onChange={(event) => setFeedback(event.target.value)}
          disabled={revisionAction?.available !== true || busy}
          rows={4}
          aria-describedby={feedbackError == null ? undefined : `${feedbackId}-error`}
          aria-invalid={feedbackError != null}
          className="w-full rounded-lg border border-[var(--border)] bg-[var(--canvas)] p-3 text-sm disabled:opacity-50"
        />
        {feedbackError != null && <p id={`${feedbackId}-error`} role="alert" className="text-sm">{feedbackError}</p>}
        <div className="flex flex-wrap gap-3">
          <button
            type="button"
            disabled={revisionAction?.available !== true || busy}
            onClick={submitRevision}
            className="rounded-lg border border-[var(--border)] px-3 py-2 disabled:opacity-50"
          >
            Request revision
          </button>
          {revisionAction?.available !== true && revisionAction?.unavailableReason != null && (
            <p className="self-center text-sm text-[var(--muted-text)]">{revisionAction.unavailableReason.message}</p>
          )}
        </div>
      </div>

      {rejectAction != null && rejectAction.available !== true && rejectAction.unavailableReason != null && (
        <p className="text-sm text-[var(--muted-text)]">
          Reject: {rejectAction.unavailableReason.message}
          <span className="ml-2 text-xs">{rejectAction.unavailableReason.code}</span>
        </p>
      )}
    </section>
  );
}
