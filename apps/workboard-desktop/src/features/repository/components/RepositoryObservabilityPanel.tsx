import { Link } from "@tanstack/react-router";

import type { RepositoryId, WorkspaceId } from "../../../core/generated";
import { useRepository } from "../hooks/useRepository";

interface RepositoryObservabilityPanelProps {
  workspaceId: WorkspaceId;
  repositoryId: RepositoryId;
}

export function RepositoryObservabilityPanel({ workspaceId, repositoryId }: RepositoryObservabilityPanelProps) {
  const repository = useRepository(workspaceId, repositoryId);
  if (repository.isLoading) return <p role="status">Loading repository evidence…</p>;
  if (repository.isDisconnected) return <PanelFailure message="Repository evidence is disconnected." onRetry={() => void repository.retry()} />;
  if (repository.error?.code === "projection_version_unavailable" || repository.error?.code === "incompatible_protocol") return <p role="alert">This daemon does not provide compatible repository observability.</p>;
  if (repository.error != null || repository.projection === undefined) return <PanelFailure message={repository.error?.message ?? "Repository evidence is unavailable."} onRetry={() => void repository.retry()} />;
  const projection = repository.projection;
  return (
    <section aria-labelledby="repository-evidence-title" className="rounded-2xl border border-[var(--border)] bg-[var(--surface)] p-5">
      <div className="flex flex-wrap items-center justify-between gap-3"><h2 id="repository-evidence-title" className="text-lg font-semibold">Repository evidence</h2>{repository.isRefreshing && <span role="status">Refreshing this panel…</span>}</div>
      {repository.isPartial && <p role="alert" className="mt-2 text-[var(--warning)]">Some repository evidence is partial.</p>}
      <dl className="mt-4 grid gap-3 sm:grid-cols-2"><Evidence label="Default branch" value={projection.defaultBranch ?? "Unknown"} state={projection.defaultBranchEvidence.state} /><Evidence label="Remotes" value={projection.remoteNames.join(", ") || "None recorded"} state={projection.remoteEvidence.state} /></dl>
      <h3 className="mt-5 font-semibold">Current and historical locations</h3>
      <ul className="mt-2 space-y-2">{projection.displayPaths.map((item) => <li key={`${item.displayPath}:${item.observedFrom}`} className="rounded-lg border border-[var(--border)] p-3"><span className="font-mono text-sm">{item.displayPath}</span><span className="ml-2 text-xs uppercase">{item.state}</span></li>)}</ul>
      <h3 className="mt-5 font-semibold">Checkouts</h3>
      {projection.checkoutIds.length === 0 ? <p className="mt-2 text-[var(--muted-text)]">No checkouts are recorded.</p> : <ul className="mt-2 flex flex-wrap gap-2">{projection.checkoutIds.map((checkoutId) => <li key={checkoutId}><Link to="/workspaces/$workspaceId/checkouts/$checkoutId" params={{ workspaceId, checkoutId }} className="rounded-lg border border-[var(--border)] px-3 py-2 text-sm">Checkout {checkoutId.slice(0, 8)}</Link></li>)}</ul>}
    </section>
  );
}

function Evidence({ label, value, state }: { label: string; value: string; state: string }) {
  return <div><dt className="text-xs font-semibold uppercase text-[var(--muted-text)]">{label}</dt><dd className="mt-1">{value} <span className="text-xs">({state.replaceAll("_", " ")})</span></dd></div>;
}

function PanelFailure({ message, onRetry }: { message: string; onRetry(): void }) {
  return <div role="alert" className="rounded-xl border border-[var(--warning-muted)] p-4"><p>{message}</p><button type="button" onClick={onRetry} className="mt-3 rounded-lg border border-[var(--border)] px-3 py-2">Retry repository panel</button></div>;
}
