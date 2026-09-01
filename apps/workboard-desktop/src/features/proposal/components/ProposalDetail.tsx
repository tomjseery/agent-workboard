import type { FeatureId, WorkspaceId } from "../../../core/generated";
import { useFeatureProposal } from "../hooks/useProposal";
import { ApprovalActions } from "./ApprovalActions";

interface ProposalDetailProps {
  workspaceId: WorkspaceId;
  featureId: FeatureId;
}

export function ProposalDetail({ workspaceId, featureId }: ProposalDetailProps) {
  const model = useFeatureProposal(workspaceId, featureId);
  if (model.isLoading) return <p role="status">Loading Feature proposal…</p>;
  if (model.isDisconnected) return <Failure message="The Feature proposal is disconnected." onRetry={() => void model.retry()} />;
  if (model.error?.code === "projection_version_unavailable" || model.error?.code === "incompatible_protocol") {
    return <p role="alert">This daemon does not provide a compatible Feature proposal projection. No proposal has been reconstructed locally.</p>;
  }
  if (model.error != null || model.projection === undefined) return <Failure message={model.error?.message ?? "The Feature proposal is unavailable."} onRetry={() => void model.retry()} />;
  const proposal = model.projection;
  return (
    <section aria-labelledby="proposal-title" className="space-y-5 rounded-2xl border border-[var(--border)] bg-[var(--surface)] p-5">
      <header className="flex flex-wrap items-start justify-between gap-3"><div><p className="text-xs font-semibold uppercase tracking-[0.18em] text-[var(--accent)]">Feature proposal</p><h2 id="proposal-title" className="mt-1 text-2xl font-semibold">{proposal.feature.title}</h2></div><span className="rounded-full border border-[var(--border)] px-3 py-1 text-xs">{proposal.workflowState.replaceAll("_", " ")}</span></header>
      {model.isRefreshing && <p role="status">Refreshing this proposal…</p>}
      {model.isStale && <p role="status">This view may be stale while Workboard refreshes authoritative evidence.</p>}
      {proposal.changedSincePrevious && <p role="alert" className="rounded-lg border border-[var(--warning-muted)] p-3">Proposal changed: review generation {proposal.generation} before acting elsewhere.</p>}
      {model.partialOutcomes.map((outcome) => <p key={outcome.code} role="alert" className="rounded-lg border border-[var(--warning-muted)] p-3">{outcome.message}{outcome.reconciliationRequired ? " Reconciliation is required." : ""}</p>)}
      <dl className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4"><Evidence label="Generation" value={String(proposal.generation)} /><Evidence label="Revision" value={String(proposal.revision)} /><Evidence label="Hash" value={proposal.proposalHash} mono /><Evidence label="Submitted" value={proposal.submittedAt} /></dl>
      <TextSection title="Proposed Feature" content={proposal.featureBody} />
      <section aria-labelledby="work-items-title"><h3 id="work-items-title" className="text-lg font-semibold">Proposed Work items</h3><ol className="mt-3 space-y-4">{proposal.workItems.map((item) => <li key={item.id} className="rounded-xl border border-[var(--border)] p-4"><h4 className="font-semibold">{item.position}. {item.title}</h4><p className="text-sm text-[var(--muted-text)]">{item.slug}</p><p className="mt-2 text-sm">Repositories: {item.repositories.map((repository) => repository.slug).join(", ") || "None"}</p><p className="text-sm">Dependencies: {item.dependencies.join(", ") || "None"}</p><pre className="mt-3 max-h-80 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-[var(--canvas)] p-3 font-sans text-sm">{item.body}</pre><a href={`/workspaces/${workspaceId}/work-items/${item.id}?q=`} className="mt-3 inline-block rounded-lg border border-[var(--border)] px-3 py-2 text-sm">Open Work item</a></li>)}</ol></section>
      <ListSection title="Verification gates" values={proposal.verificationGates} empty="No verification gates were supplied." />
      <section aria-labelledby="proposal-warnings"><h3 id="proposal-warnings" className="text-lg font-semibold">Warnings</h3>{proposal.warnings.length === 0 ? <p className="mt-2">No warnings.</p> : <ul className="mt-2 space-y-2">{proposal.warnings.map((warning) => <li key={warning.code} role={warning.severity === "error" || warning.severity === "warning" ? "alert" : undefined} className="rounded-lg border border-[var(--warning-muted)] p-3"><span className="font-semibold">{warning.code.replaceAll("_", " ")}:</span> {warning.message}</li>)}</ul>}</section>
      <section aria-labelledby="planner-sessions"><h3 id="planner-sessions" className="text-lg font-semibold">Planner sessions</h3>{proposal.plannerSessions.length === 0 ? <p className="mt-2">No planner sessions are recorded.</p> : <ul className="mt-2 grid gap-2 sm:grid-cols-2">{proposal.plannerSessions.map((session) => <li key={session.id} className="rounded-lg border border-[var(--border)] p-3"><span className="font-semibold">{session.provider}</span><span className="ml-2">{session.liveState.replaceAll("_", " ")}</span><p className="text-xs text-[var(--muted-text)]">{session.bindingState.replaceAll("_", " ")}</p></li>)}</ul>}</section>
      {(proposal.diagnostics.length > 0 || model.diagnostics.length > 0) && <section aria-labelledby="proposal-diagnostics"><h3 id="proposal-diagnostics" className="text-lg font-semibold">Diagnostics</h3><ul className="mt-2 space-y-2">{[...proposal.diagnostics, ...model.diagnostics].map((diagnostic) => <li key={`${diagnostic.code}:${diagnostic.message}`} role="alert" className="rounded-lg border border-[var(--warning-muted)] p-3">{diagnostic.message}</li>)}</ul></section>}
      <ApprovalActions workspaceId={workspaceId} featureId={featureId} actions={proposal.availableActions} revision={proposal.revision} />
    </section>
  );
}

function TextSection({ title, content }: { title: string; content: string }) {
  return <section><h3 className="text-lg font-semibold">{title}</h3><pre className="mt-2 max-h-[36rem] overflow-auto whitespace-pre-wrap break-words rounded-xl bg-[var(--canvas)] p-4 font-sans text-sm">{content}</pre></section>;
}

function ListSection({ title, values, empty }: { title: string; values: string[]; empty: string }) {
  return <section><h3 className="text-lg font-semibold">{title}</h3>{values.length === 0 ? <p className="mt-2">{empty}</p> : <ul className="mt-2 list-disc space-y-1 pl-5">{values.map((value, index) => <li key={`${index}:${value}`}>{value}</li>)}</ul>}</section>;
}

function Evidence({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return <div><dt className="text-xs font-semibold uppercase text-[var(--muted-text)]">{label}</dt><dd className={`mt-1 break-all text-sm${mono ? " font-mono" : ""}`}>{value}</dd></div>;
}

function Failure({ message, onRetry }: { message: string; onRetry(): void }) {
  return <div role="alert" className="rounded-xl border border-[var(--warning-muted)] p-4"><p>{message}</p><button type="button" onClick={onRetry} className="mt-3 rounded-lg border border-[var(--border)] px-3 py-2">Retry proposal detail</button></div>;
}
