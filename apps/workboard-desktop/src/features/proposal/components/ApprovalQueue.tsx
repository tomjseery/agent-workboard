import { Link } from "@tanstack/react-router";

import type { WorkspaceId } from "../../../core/generated";
import { useApprovalQueue } from "../hooks/useProposal";

export function ApprovalQueue({ workspaceId }: { workspaceId: WorkspaceId }) {
  const model = useApprovalQueue(workspaceId);
  if (model.isLoading) return <p role="status">Loading approval queue…</p>;
  if (model.isDisconnected) return <Failure message="The approval queue is disconnected." onRetry={() => void model.retry()} />;
  if (model.error?.code === "projection_version_unavailable" || model.error?.code === "incompatible_protocol") return <p role="alert">This daemon does not provide a compatible approval queue. No local queue has been inferred.</p>;
  if (model.error != null || model.projection === undefined) return <Failure message={model.error?.message ?? "The approval queue is unavailable."} onRetry={() => void model.retry()} />;
  if (model.projection.entries.length === 0) return <p>No Feature proposals currently require review.</p>;
  return <section aria-labelledby="approval-queue-title" className="space-y-4"><header><p className="text-xs font-semibold uppercase tracking-[0.18em] text-[var(--accent)]">Daemon-owned ordering</p><h1 id="approval-queue-title" className="mt-1 text-3xl font-semibold">Feature proposal reviews</h1></header>{model.isRefreshing && <p role="status">Refreshing the approval queue…</p>}{model.isStale && <p role="status">The queue may be stale while Workboard refreshes.</p>}{model.partialOutcomes.map((outcome) => <p key={outcome.code} role="alert" className="rounded-lg border border-[var(--warning-muted)] p-3">{outcome.message}</p>)}<ol className="grid gap-3">{model.projection.entries.map((entry) => <li key={entry.feature.id} aria-posinset={entry.position} aria-setsize={entry.totalCount} className="rounded-xl border border-[var(--border)] bg-[var(--surface)] p-4"><div className="flex flex-wrap items-start justify-between gap-3"><div><h2 className="font-semibold"><Link to="/workspaces/$workspaceId/features/$featureId/proposal" params={{ workspaceId, featureId: entry.feature.id }} className="underline decoration-[var(--border)] underline-offset-4">{entry.feature.title}</Link></h2><p className="text-sm text-[var(--muted-text)]">Generation {entry.generation} · revision {entry.revision}</p></div><span className="rounded-full border border-[var(--border)] px-3 py-1 text-xs">{entry.workflowState.replaceAll("_", " ")}</span></div>{entry.changedSincePrevious && <p role="alert" className="mt-3 text-[var(--warning)]">Changed proposal — review the new hash.</p>}<p className="mt-3 text-sm">{entry.repositories.map((repository) => repository.slug).join(", ") || "No repository scope"}</p><p className="mt-1 text-xs text-[var(--muted-text)]">{entry.warningCount} warnings · {entry.plannerCount} planners</p><p className="mt-1 break-all font-mono text-xs">{entry.proposalHash}</p></li>)}</ol></section>;
}

function Failure({ message, onRetry }: { message: string; onRetry(): void }) {
  return <div role="alert" className="rounded-xl border border-[var(--warning-muted)] p-4"><p>{message}</p><button type="button" onClick={onRetry} className="mt-3 rounded-lg border border-[var(--border)] px-3 py-2">Retry approval queue</button></div>;
}
