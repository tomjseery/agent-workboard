import { Link } from "@tanstack/react-router";

import { Alert } from "../../../components/ui/alert";
import { buttonVariants, Button } from "../../../components/ui/button";
import { Card, CardTitle } from "../../../components/ui/card";
import { RetryAlert } from "../../../components/ui/retry-alert";
import type {
  PrimaryWriterEvidence,
  RecoveryDisposition,
  SessionBindingState,
  SessionId,
  SessionLiveState,
  SessionRestoreState,
  SessionResumability,
  WorkspaceId,
} from "../../../core/contracts";
import { formatTimestamp } from "../../../lib/dates";
import { useRecoveryPreview, useSession } from "../hooks/useSession";

const liveLabels: Record<SessionLiveState, string> = {
  active: "Active",
  idle: "Idle",
  stopped: "Stopped",
  unknown: "Unknown",
  system_error: "System error",
  not_loaded: "Not loaded",
};
const bindingLabels: Record<SessionBindingState, string> = {
  pending: "Pending",
  current: "Current",
  stopped: "Stopped",
  reconciliation_required: "Reconciliation required",
};
const restoreLabels: Record<SessionRestoreState, string> = {
  tracked: "Tracked",
  removed: "Removed",
  not_tracked: "Not tracked",
  conflict: "Conflict",
};
const resumabilityLabels: Record<SessionResumability, string> = {
  validated: "Validated",
  preflight_passed: "Preflight passed",
  unknown: "Unknown",
  missing: "Missing",
  corrupt: "Corrupt",
  unsupported: "Unsupported",
};
const writerLabels: Record<PrimaryWriterEvidence, string> = {
  confirmed_primary: "Confirmed primary",
  confirmed_secondary: "Confirmed secondary",
  not_applicable: "Not applicable",
  unknown: "Unknown",
  conflict: "Conflict",
};
const recoveryLabels: Record<RecoveryDisposition, string> = {
  ready_present: "Ready with current checkout",
  ready_recreate: "Ready after checkout recreation",
  already_live: "Already live",
  conflict: "Recovery conflict",
  unresumable: "Unresumable",
  not_loaded: "Not loaded",
};

export function SessionDetail({ workspaceId, sessionId }: { workspaceId: WorkspaceId; sessionId: SessionId }) {
  const session = useSession(workspaceId, sessionId);
  if (session.isLoading) return <p role="status">Loading session evidence...</p>;
  if (session.isDisconnected) return <RetryAlert message="Session evidence is disconnected." actionLabel="Retry session panel" onRetry={() => void session.retry()} />;
  if (session.error?.code === "projection_version_unavailable" || session.error?.code === "incompatible_protocol") {
    return <p role="alert">This daemon does not provide compatible session observability.</p>;
  }
  if (session.error != null || session.projection === undefined) {
    return <RetryAlert message={session.error?.message ?? "Session evidence is unavailable."} actionLabel="Retry session panel" onRetry={() => void session.retry()} />;
  }

  const projection = session.projection;

  return (
    <div className="space-y-5">
      <header>
        <p className="text-sm text-muted-foreground">Workboard session {projection.id}</p>
        <h1 className="text-2xl font-semibold">{projection.provider} {projection.role.replaceAll("_", " ")}</h1>
      </header>
      {projection.liveness.stale && (
        <Alert size="lg" className="rounded-xl">Liveness evidence is stale. The session is {liveLabels[projection.liveness.state]}, not assumed stopped.</Alert>
      )}
      {session.isRefreshing && <p role="status">Refreshing this session...</p>}
      {session.isPartial && <p role="alert">Some session evidence is partial.</p>}

      <Card asChild>
        <section aria-labelledby="session-state">
          <CardTitle id="session-state">Authoritative state</CardTitle>
          <dl className="mt-3 grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            <Item label="Live state" value={liveLabels[projection.liveness.state]} />
            <Item label="Binding" value={bindingLabels[projection.bindingState]} />
            <Item label="Restore" value={restoreLabels[projection.restoreState]} />
            <Item label="Resumability" value={resumabilityLabels[projection.resumability]} />
            <Item label="Primary writer" value={writerLabels[projection.primaryWriter]} />
            <Item label="Last activity" value={projection.lastActivityAt == null ? "Not loaded" : formatTimestamp(projection.lastActivityAt)} />
          </dl>
          <p className="mt-4 text-sm">{projection.liveness.evidence.message}</p>
        </section>
      </Card>

      <Card asChild>
        <section aria-labelledby="session-profile">
          <CardTitle id="session-profile">Profile and binding evidence</CardTitle>
          <dl className="mt-3 grid gap-3 sm:grid-cols-2">
            <Item label="Authoritative profile" value={projection.authoritativeProfile ?? "Not loaded"} />
            <Item label="Authoritative model" value={projection.authoritativeModel ?? "Not loaded"} />
            <Item label="Owner" value={`${projection.owner.kind} ${projection.owner.id}`} />
            <Item label="Profile evidence" value={projection.profileEvidence.message} />
          </dl>
          {projection.checkoutId != null && (
            <Link to="/workspaces/$workspaceId/checkouts/$checkoutId" params={{ workspaceId, checkoutId: projection.checkoutId }} className={`${buttonVariants()} mt-4`}>
              Open checkout evidence
            </Link>
          )}
          {projection.owner.kind === "work_item" && (
            <Link to="/workspaces/$workspaceId/work-items/$workItemId" params={{ workspaceId, workItemId: projection.owner.id }} className={`${buttonVariants()} mt-4 ml-2`}>
              Open Work item
            </Link>
          )}
        </section>
      </Card>

      <RecoveryPanel workspaceId={workspaceId} sessionId={sessionId} />
    </div>
  );
}

function RecoveryPanel({ workspaceId, sessionId }: { workspaceId: WorkspaceId; sessionId: SessionId }) {
  const recovery = useRecoveryPreview(workspaceId, sessionId);
  if (recovery.isLoading) return <section aria-label="Recovery preview"><p role="status">Loading recovery preview...</p></section>;
  if (recovery.isDisconnected) return <RetryAlert message="Recovery preview is disconnected." actionLabel="Retry recovery panel" onRetry={() => void recovery.retry()} />;
  if (recovery.error != null || recovery.projection === undefined) {
    return <RetryAlert message={recovery.error?.message ?? "Recovery preview is unavailable."} actionLabel="Retry recovery panel" onRetry={() => void recovery.retry()} />;
  }

  const projection = recovery.projection;

  return (
    <Card asChild>
      <section aria-labelledby="recovery-preview">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <CardTitle id="recovery-preview">Recovery preview</CardTitle>
          {recovery.isRefreshing && <span role="status">Refreshing recovery preview...</span>}
        </div>
        {projection.stale && <p role="alert" className="mt-3">Recovery evidence is stale.</p>}
        {recovery.isPartial && <p role="alert" className="mt-3">Some recovery evidence is partial.</p>}
        <p className="mt-3 text-lg font-medium">{recoveryLabels[projection.disposition]}</p>
        {projection.conflicts.length > 0 && (
          <ul className="mt-3 space-y-2">
            {projection.conflicts.map((conflict) => (
              <li key={conflict.code}>
                <Alert>{conflict.message}</Alert>
              </li>
            ))}
          </ul>
        )}
        <Button type="button" onClick={() => void recovery.retry()} className="mt-4">Refresh recovery panel</Button>
      </section>
    </Card>
  );
}

function Item({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt className="text-xs font-semibold text-muted-foreground uppercase">{label}</dt>
      <dd className="mt-1 break-words">{value}</dd>
    </div>
  );
}
