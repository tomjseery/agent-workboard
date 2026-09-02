import { Link } from "@tanstack/react-router";

import type { DurableWorkItemSection, WorkItemId, WorkspaceId } from "../../../core/generated";
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

export function WorkItemDetail({ workspaceId, workItemId }: { workspaceId: WorkspaceId; workItemId: WorkItemId }) {
  const model = useWorkItemDetail(workspaceId, workItemId);
  if (model.isLoading) return <p role="status">Loading Work-item detail...</p>;
  if (model.isDisconnected) return <Failure message="Work-item detail is disconnected." retry={() => void model.retry()} />;
  if (model.error?.code === "projection_version_unavailable" || model.error?.code === "incompatible_protocol") return <p role="alert">This daemon does not provide a compatible Work-item detail projection. No durable state has been reconstructed locally.</p>;
  if (model.error != null || model.projection === undefined) return <Failure message={model.error?.message ?? "Work-item detail is unavailable."} retry={() => void model.retry()} />;
  const detail = model.projection;
  const checkpointGate = detail.availableActions.find((action) => action.code === "checkpoint_work_item")?.unavailableReason;
  const diagnostics = [...detail.diagnostics, ...model.diagnostics];
  return (
    <article className="min-w-0 space-y-6">
      <header className="rounded-2xl border border-[var(--border)] bg-[var(--surface)] p-5">
        <p className="text-xs font-semibold uppercase tracking-[0.18em] text-[var(--accent)]">Work item {detail.workItem.key}</p>
        <div className="mt-2 flex flex-wrap items-start justify-between gap-3"><div><h1 className="text-3xl font-semibold break-words">{detail.workItem.title}</h1><p className="mt-1 text-[var(--muted-text)]">{detail.feature.title}</p></div><div className="flex flex-wrap gap-2 text-xs"><span className="rounded-full border border-[var(--border)] px-3 py-1">{detail.status.replaceAll("_", " ")}</span><span className="rounded-full border border-[var(--border)] px-3 py-1">{detail.dependencyReadiness.replaceAll("_", " ")}</span></div></div>
        <dl className="mt-5 grid gap-3 sm:grid-cols-2 lg:grid-cols-4"><Evidence label="Revision" value={String(detail.revision)} /><Evidence label="Content revision" value={String(detail.contentRevision)} /><Evidence label="Content hash" value={detail.contentHash} mono /><Evidence label="Workflow" value={detail.workflowState.replaceAll("_", " ")} /></dl>
      </header>
      {model.isRefreshing && <p role="status">Refreshing authoritative Work-item detail...</p>}
      {model.isStale && <p role="status">This detail may be stale while Workboard refreshes authoritative evidence.</p>}
      {model.partialOutcomes.map((outcome) => <p key={`${outcome.code}:${outcome.message}`} role="alert" className="rounded-xl border border-[var(--warning-muted)] p-3">{outcome.message}{outcome.reconciliationRequired ? " Reconciliation is required." : ""}</p>)}
      <nav aria-label="Work-item sections" className="rounded-xl border border-[var(--border)] bg-[var(--surface)] p-3"><ul className="flex flex-wrap gap-2">{sections.map(([id, label]) => <li key={id}><a href={`#${id}`} className="inline-block rounded-lg border border-[var(--border)] px-3 py-2 text-sm focus:outline-2">{label}</a></li>)}</ul></nav>
      <TextSection id="outcome" title="Outcome and design" content={detail.outcomeDesignSummary} empty="No outcome or design summary is recorded." />
      <DurableSection id="state" title="Current state" section={detail.currentState} empty="No structured current state is recorded." />
      <section id="dependencies" tabIndex={-1} aria-labelledby="dependencies-title" className="scroll-mt-6 rounded-xl border border-[var(--border)] bg-[var(--surface)] p-5"><h2 id="dependencies-title" className="text-lg font-semibold">Dependencies and blockers</h2><p className="mt-2">Readiness: {detail.dependencyReadiness.replaceAll("_", " ")}</p>{detail.blockers.length === 0 ? <p className="mt-2 text-[var(--muted-text)]">No authoritative blockers are recorded.</p> : <ul className="mt-3 space-y-2">{detail.blockers.map((blocker, index) => <li key={`${blocker.code}:${index}`} role="alert" className="rounded-lg border border-[var(--warning-muted)] p-3"><p>{blocker.message}</p>{blocker.prerequisite != null && <Link to="/workspaces/$workspaceId/work-items/$workItemId" params={{ workspaceId, workItemId: blocker.prerequisite.id }} className="mt-2 inline-block text-sm underline">Open {blocker.prerequisite.title}</Link>}</li>)}</ul>}</section>
      <DurableSection id="decisions" title="Decisions" section={detail.decisions} empty="No structured decisions are recorded." />
      <DurableSection id="verification" title="Verification" section={detail.verification} empty="No structured verification evidence is recorded." />
      <section id="next-action" tabIndex={-1} aria-labelledby="next-action-title" className="scroll-mt-6 rounded-xl border border-[var(--border)] bg-[var(--surface)] p-5"><h2 id="next-action-title" className="text-lg font-semibold">Next action and delivery</h2>{detail.nextAction == null ? <p className="mt-2">No next action is recorded.</p> : <dl className="mt-3 grid gap-3 sm:grid-cols-3"><Evidence label="Next action" value={detail.nextAction.kind.replaceAll("_", " ")} /><Evidence label="Recorded" value={detail.nextAction.recordedAt} /><Evidence label="Review/delivery" value={detail.reviewDeliveryState.replaceAll("_", " ")} /></dl>}</section>
      <section id="checkpoints" tabIndex={-1} aria-labelledby="checkpoints-title" className="scroll-mt-6 rounded-xl border border-[var(--border)] bg-[var(--surface)] p-5"><h2 id="checkpoints-title" className="text-lg font-semibold">Checkpoint history</h2>{detail.checkpointHistory.length === 0 ? <p className="mt-2">No checkpoints are recorded.</p> : <ol className="mt-3 space-y-3">{detail.checkpointHistory.map((checkpoint) => <li key={checkpoint.id} className="rounded-lg border border-[var(--border)] p-3"><div className="flex flex-wrap justify-between gap-2 text-sm"><strong>{checkpoint.nextAction.replaceAll("_", " ")}</strong><span>{checkpoint.recordedAt}</span></div><pre className="mt-2 max-h-80 overflow-auto whitespace-pre-wrap break-words font-sans text-sm">{checkpoint.summary}</pre><Link to="/workspaces/$workspaceId/sessions/$sessionId" params={{ workspaceId, sessionId: checkpoint.sessionId }} className="mt-2 inline-block text-sm underline">Open recording session</Link></li>)}</ol>}</section>
      <section id="resources" tabIndex={-1} aria-labelledby="resources-title" className="scroll-mt-6 rounded-xl border border-[var(--border)] bg-[var(--surface)] p-5"><h2 id="resources-title" className="text-lg font-semibold">Repositories and checkouts</h2><ul className="mt-3 flex flex-wrap gap-2">{detail.repositories.map((repository) => <li key={repository.id}><Link to="/workspaces/$workspaceId/repositories/$repositoryId" params={{ workspaceId, repositoryId: repository.id }} search={{ view: "board" }} className="inline-block rounded-lg border border-[var(--border)] px-3 py-2">{repository.title}</Link></li>)}</ul>{detail.checkouts.length === 0 ? <p className="mt-3">No effective checkouts are recorded.</p> : <ul className="mt-3 grid gap-2 sm:grid-cols-2">{detail.checkouts.map((checkout) => <li key={checkout.id}><Link to="/workspaces/$workspaceId/checkouts/$checkoutId" params={{ workspaceId, checkoutId: checkout.id }} className="block rounded-lg border border-[var(--border)] p-3"><strong>{checkout.repository.title}</strong><span className="block text-sm">{checkout.availability} · {checkout.purpose.replaceAll("_", " ")}</span></Link></li>)}</ul>}</section>
      <section id="sessions" tabIndex={-1} aria-labelledby="sessions-title" className="scroll-mt-6 rounded-xl border border-[var(--border)] bg-[var(--surface)] p-5"><h2 id="sessions-title" className="text-lg font-semibold">Sessions</h2>{detail.sessions.length === 0 ? <p className="mt-2">No bound sessions are recorded.</p> : <ul className="mt-3 grid gap-2 sm:grid-cols-2">{detail.sessions.map((session) => <li key={session.id}><Link to="/workspaces/$workspaceId/sessions/$sessionId" params={{ workspaceId, sessionId: session.id }} className="block rounded-lg border border-[var(--border)] p-3"><strong>{session.provider} {session.role.replaceAll("_", " ")}</strong><span className="block text-sm">{session.liveness.state.replaceAll("_", " ")}{session.liveness.stale ? " · stale evidence" : ""}</span></Link></li>)}</ul>}</section>
      <SessionControls workspaceId={workspaceId} workItemId={workItemId} sessions={detail.sessions} repositories={detail.repositories} actions={detail.availableActions} revision={detail.revision} />
      <section id="diagnostics" tabIndex={-1} aria-labelledby="diagnostics-title" className="scroll-mt-6 rounded-xl border border-[var(--border)] bg-[var(--surface)] p-5"><h2 id="diagnostics-title" className="text-lg font-semibold">Diagnostics</h2>{diagnostics.length === 0 ? <p className="mt-2">No diagnostics are recorded.</p> : <ul className="mt-3 space-y-2">{diagnostics.map((diagnostic) => <li key={`${diagnostic.code}:${diagnostic.message}`} role={diagnostic.severity === "error" || diagnostic.severity === "warning" ? "alert" : undefined} className="rounded-lg border border-[var(--warning-muted)] p-3"><strong>{diagnostic.code.replaceAll("_", " ")}</strong><p>{diagnostic.message}</p></li>)}</ul>}</section>
      {checkpointGate != null && <aside aria-label="Checkpoint availability" className="rounded-xl border border-[var(--border)] bg-[var(--surface)] p-5"><h2 className="font-semibold">Structured checkpoints unavailable</h2><p className="mt-1">{checkpointGate.message}</p><p className="mt-1 text-xs text-[var(--muted-text)]">{checkpointGate.code}</p></aside>}
    </article>
  );
}

function TextSection({ id, title, content, empty }: { id: string; title: string; content: string; empty: string }) {
  return <section id={id} tabIndex={-1} aria-labelledby={`${id}-title`} className="scroll-mt-6 rounded-xl border border-[var(--border)] bg-[var(--surface)] p-5"><h2 id={`${id}-title`} className="text-lg font-semibold">{title}</h2>{content.length === 0 ? <p className="mt-2">{empty}</p> : <pre className="mt-3 max-h-[36rem] overflow-auto whitespace-pre-wrap break-words font-sans text-sm">{content}</pre>}</section>;
}

function DurableSection({ id, title, section, empty }: { id: string; title: string; section: DurableWorkItemSection; empty: string }) {
  return <section id={id} tabIndex={-1} aria-labelledby={`${id}-title`} className="scroll-mt-6 rounded-xl border border-[var(--border)] bg-[var(--surface)] p-5"><h2 id={`${id}-title`} className="text-lg font-semibold">{title}</h2>{section.entries.length === 0 ? <p className="mt-2">{empty}</p> : <ul className="mt-3 list-disc space-y-1 pl-5">{section.entries.map((entry, index) => <li key={`${index}:${entry}`}>{entry}</li>)}</ul>}<p className="mt-3 text-sm text-[var(--muted-text)]">{section.evidence.message}</p></section>;
}

function Evidence({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return <div><dt className="text-xs font-semibold uppercase text-[var(--muted-text)]">{label}</dt><dd className={`mt-1 break-all${mono ? " font-mono text-sm" : ""}`}>{value}</dd></div>;
}

function Failure({ message, retry }: { message: string; retry(): void }) {
  return <div role="alert" className="rounded-xl border border-[var(--warning-muted)] p-4"><p>{message}</p><button type="button" onClick={retry} className="mt-3 rounded-lg border border-[var(--border)] px-3 py-2">Retry Work-item detail</button></div>;
}
