import { Link } from "@tanstack/react-router";
import type { ReactNode } from "react";

import { Alert } from "../../../components/ui/alert";
import { Badge } from "../../../components/ui/badge";
import { buttonVariants } from "../../../components/ui/button";
import { Card, CardEyebrow, CardTitle } from "../../../components/ui/card";
import { RetryAlert } from "../../../components/ui/retry-alert";
import type { DurableWorkItemSection, WorkItemId, WorkspaceId } from "../../../core/contracts";
import { formatTimestamp } from "../../../lib/dates";
import { useWorkItemDetail } from "../hooks/useWorkItem";
import { SessionControls } from "./SessionControls";

const sections = [
  ["outcome", "Outcome and design"],
  ["state", "Current state"],
  ["dependencies", "Dependencies and blockers"],
  ["decisions", "Decisions"],
  ["verification", "Verification"],
  ["next-action", "Next action"],
  ["checkpoints", "Checkpoint history"],
  ["resources", "Repositories and checkouts"],
  ["sessions", "Sessions"],
  ["session-controls", "Session controls"],
  ["diagnostics", "Diagnostics"],
] as const;

const retryLabel = "Retry Work-item detail";

export function WorkItemDetail({ workspaceId, workItemId }: { workspaceId: WorkspaceId; workItemId: WorkItemId }) {
  const model = useWorkItemDetail(workspaceId, workItemId);
  if (model.isLoading) return <p role="status">Loading Work-item detail...</p>;
  if (model.isDisconnected) return <RetryAlert message="Work-item detail is disconnected." actionLabel={retryLabel} onRetry={() => void model.retry()} />;
  if (model.error?.code === "projection_version_unavailable" || model.error?.code === "incompatible_protocol") return <p role="alert">This daemon does not provide a compatible Work-item detail projection. No durable state has been reconstructed locally.</p>;
  if (model.error != null || model.projection === undefined) return <RetryAlert message={model.error?.message ?? "Work-item detail is unavailable."} actionLabel={retryLabel} onRetry={() => void model.retry()} />;

  const detail = model.projection;
  const checkpointGate = detail.availableActions.find((action) => action.code === "checkpoint_work_item")?.unavailableReason;
  const diagnostics = [...detail.diagnostics, ...model.diagnostics];

  return (
    <article className="min-w-0 space-y-6">
      <Card asChild size="compact" className="rounded-2xl p-5">
        <header>
          <CardEyebrow>Work item {detail.workItem.key}</CardEyebrow>
          <div className="mt-2 flex flex-wrap items-start justify-between gap-3">
            <div>
              <h1 className="text-3xl font-semibold break-words">{detail.workItem.title}</h1>
              <p className="mt-1 text-muted-foreground">{detail.feature.title}</p>
            </div>
            <div className="flex flex-wrap gap-2">
              <Badge size="lg">{detail.status.replaceAll("_", " ")}</Badge>
              <Badge size="lg">{detail.dependencyReadiness.replaceAll("_", " ")}</Badge>
            </div>
          </div>
          <dl className="mt-5 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
            <Evidence label="Revision" value={String(detail.revision)} />
            <Evidence label="Content revision" value={String(detail.contentRevision)} />
            <Evidence label="Content hash" value={detail.contentHash} mono />
            <Evidence label="Workflow" value={detail.workflowState.replaceAll("_", " ")} />
          </dl>
        </header>
      </Card>

      {model.isRefreshing && <p role="status">Refreshing authoritative Work-item detail...</p>}
      {model.isStale && <p role="status">This detail may be stale while Workboard refreshes authoritative evidence.</p>}
      {model.partialOutcomes.map((outcome) => (
        <Alert key={`${outcome.code}:${outcome.message}`} size="lg" className="rounded-xl">
          {outcome.message}{outcome.reconciliationRequired ? " Reconciliation is required." : ""}
        </Alert>
      ))}

      <Card asChild size="compact" className="p-3">
        <nav aria-label="Work-item sections">
          <ul className="flex flex-wrap gap-2">
            {sections.map(([id, label]) => (
              <li key={id}>
                <a href={`#${id}`} className={buttonVariants()}>{label}</a>
              </li>
            ))}
          </ul>
        </nav>
      </Card>

      <TextSection id="outcome" title="Outcome and design" content={detail.outcomeDesignSummary} empty="No outcome or design summary is recorded." />
      <DurableSection id="state" title="Current state" section={detail.currentState} empty="No structured current state is recorded." />

      <DetailSection id="dependencies" title="Dependencies and blockers">
        <p className="mt-2">Readiness: {detail.dependencyReadiness.replaceAll("_", " ")}</p>
        {detail.blockers.length === 0 ? (
          <p className="mt-2 text-muted-foreground">No authoritative blockers are recorded.</p>
        ) : (
          <ul className="mt-3 space-y-2">
            {detail.blockers.map((blocker, index) => (
              <li key={`${blocker.code}:${index}`}>
                <Alert>
                  <p>{blocker.message}</p>
                  {blocker.prerequisite != null && (
                    <Link to="/workspaces/$workspaceId/work-items/$workItemId" params={{ workspaceId, workItemId: blocker.prerequisite.id }} className="mt-2 inline-block text-sm underline">
                      Open {blocker.prerequisite.title}
                    </Link>
                  )}
                </Alert>
              </li>
            ))}
          </ul>
        )}
      </DetailSection>

      <DurableSection id="decisions" title="Decisions" section={detail.decisions} empty="No structured decisions are recorded." />
      <DurableSection id="verification" title="Verification" section={detail.verification} empty="No structured verification evidence is recorded." />

      <DetailSection id="next-action" title="Next action and delivery">
        {detail.nextAction == null ? (
          <p className="mt-2">No next action is recorded.</p>
        ) : (
          <dl className="mt-3 grid gap-3 sm:grid-cols-3">
            <Evidence label="Next action" value={detail.nextAction.kind.replaceAll("_", " ")} />
            <Evidence label="Recorded" value={formatTimestamp(detail.nextAction.recordedAt)} />
            <Evidence label="Review/delivery" value={detail.reviewDeliveryState.replaceAll("_", " ")} />
          </dl>
        )}
      </DetailSection>

      <DetailSection id="checkpoints" title="Checkpoint history">
        {detail.checkpointHistory.length === 0 ? (
          <p className="mt-2">No checkpoints are recorded.</p>
        ) : (
          <ol className="mt-3 space-y-3">
            {detail.checkpointHistory.map((checkpoint) => (
              <li key={checkpoint.id} className="rounded-lg border border-border p-3">
                <div className="flex flex-wrap justify-between gap-2 text-sm">
                  <strong>{checkpoint.nextAction.replaceAll("_", " ")}</strong>
                  <span>{formatTimestamp(checkpoint.recordedAt)}</span>
                </div>
                <pre className="mt-2 max-h-80 overflow-auto font-sans text-sm break-words whitespace-pre-wrap">{checkpoint.summary}</pre>
                <Link to="/workspaces/$workspaceId/sessions/$sessionId" params={{ workspaceId, sessionId: checkpoint.sessionId }} className="mt-2 inline-block text-sm underline">
                  Open recording session
                </Link>
              </li>
            ))}
          </ol>
        )}
      </DetailSection>

      <DetailSection id="resources" title="Repositories and checkouts">
        <ul className="mt-3 flex flex-wrap gap-2">
          {detail.repositories.map((repository) => (
            <li key={repository.id}>
              <Link to="/workspaces/$workspaceId/repositories/$repositoryId" params={{ workspaceId, repositoryId: repository.id }} search={{ view: "board" }} className={buttonVariants()}>
                {repository.title}
              </Link>
            </li>
          ))}
        </ul>
        {detail.checkouts.length === 0 ? (
          <p className="mt-3">No effective checkouts are recorded.</p>
        ) : (
          <ul className="mt-3 grid gap-2 sm:grid-cols-2">
            {detail.checkouts.map((checkout) => (
              <li key={checkout.id}>
                <Link to="/workspaces/$workspaceId/checkouts/$checkoutId" params={{ workspaceId, checkoutId: checkout.id }} className="block rounded-lg border border-border p-3">
                  <strong>{checkout.repository.title}</strong>
                  <span className="block text-sm">{checkout.availability} · {checkout.purpose.replaceAll("_", " ")}</span>
                </Link>
              </li>
            ))}
          </ul>
        )}
      </DetailSection>

      <DetailSection id="sessions" title="Sessions">
        {detail.sessions.length === 0 ? (
          <p className="mt-2">No bound sessions are recorded.</p>
        ) : (
          <ul className="mt-3 grid gap-2 sm:grid-cols-2">
            {detail.sessions.map((session) => (
              <li key={session.id}>
                <Link to="/workspaces/$workspaceId/sessions/$sessionId" params={{ workspaceId, sessionId: session.id }} className="block rounded-lg border border-border p-3">
                  <strong>{session.provider} {session.role.replaceAll("_", " ")}</strong>
                  <span className="block text-sm">{session.liveness.state.replaceAll("_", " ")}{session.liveness.stale ? " · stale evidence" : ""}</span>
                </Link>
              </li>
            ))}
          </ul>
        )}
      </DetailSection>

      <SessionControls workspaceId={workspaceId} workItemId={workItemId} sessions={detail.sessions} repositories={detail.repositories} actions={detail.availableActions} revision={detail.revision} />

      <DetailSection id="diagnostics" title="Diagnostics">
        {diagnostics.length === 0 ? (
          <p className="mt-2">No diagnostics are recorded.</p>
        ) : (
          <ul className="mt-3 space-y-2">
            {diagnostics.map((diagnostic) => (
              <li key={`${diagnostic.code}:${diagnostic.message}`}>
                <Alert role={diagnostic.severity === "error" || diagnostic.severity === "warning" ? "alert" : undefined}>
                  <strong>{diagnostic.code.replaceAll("_", " ")}</strong>
                  <p>{diagnostic.message}</p>
                </Alert>
              </li>
            ))}
          </ul>
        )}
      </DetailSection>

      {checkpointGate != null && (
        <Card asChild size="compact" className="p-5">
          <aside aria-label="Checkpoint availability">
            <h2 className="font-semibold">Structured checkpoints unavailable</h2>
            <p className="mt-1">{checkpointGate.message}</p>
            <p className="mt-1 text-xs text-muted-foreground">{checkpointGate.code}</p>
          </aside>
        </Card>
      )}
    </article>
  );
}

function DetailSection({ id, title, children }: { id: string; title: string; children: ReactNode }) {
  return (
    <Card asChild size="compact" className="p-5">
      <section id={id} tabIndex={-1} aria-labelledby={`${id}-title`} className="scroll-mt-6">
        <CardTitle id={`${id}-title`}>{title}</CardTitle>
        {children}
      </section>
    </Card>
  );
}

function TextSection({ id, title, content, empty }: { id: string; title: string; content: string; empty: string }) {
  return (
    <DetailSection id={id} title={title}>
      {content.length === 0 ? <p className="mt-2">{empty}</p> : <pre className="mt-3 max-h-[36rem] overflow-auto font-sans text-sm break-words whitespace-pre-wrap">{content}</pre>}
    </DetailSection>
  );
}

function DurableSection({ id, title, section, empty }: { id: string; title: string; section: DurableWorkItemSection; empty: string }) {
  return (
    <DetailSection id={id} title={title}>
      {section.entries.length === 0 ? (
        <p className="mt-2">{empty}</p>
      ) : (
        <ul className="mt-3 list-disc space-y-1 pl-5">{section.entries.map((entry, index) => <li key={`${index}:${entry}`}>{entry}</li>)}</ul>
      )}
      <p className="mt-3 text-sm text-muted-foreground">{section.evidence.message}</p>
    </DetailSection>
  );
}

function Evidence({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return (
    <div>
      <dt className="text-xs font-semibold text-muted-foreground uppercase">{label}</dt>
      <dd className={mono ? "mt-1 font-mono text-sm break-all" : "mt-1 break-all"}>{value}</dd>
    </div>
  );
}
