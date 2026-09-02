import { Link } from "@tanstack/react-router";

import { buttonVariants } from "../../../components/ui/button";
import { Card, CardTitle } from "../../../components/ui/card";
import { RetryAlert } from "../../../components/ui/retry-alert";
import type { RepositoryId, WorkspaceId } from "../../../core/contracts";
import { useRepository } from "../hooks/useRepository";

interface RepositoryObservabilityPanelProps {
  workspaceId: WorkspaceId;
  repositoryId: RepositoryId;
}

const retryLabel = "Retry repository panel";

export function RepositoryObservabilityPanel({ workspaceId, repositoryId }: RepositoryObservabilityPanelProps) {
  const repository = useRepository(workspaceId, repositoryId);
  if (repository.isLoading) return <p role="status">Loading repository evidence…</p>;
  if (repository.isDisconnected) return <RetryAlert message="Repository evidence is disconnected." actionLabel={retryLabel} onRetry={() => void repository.retry()} />;
  if (repository.error?.code === "projection_version_unavailable" || repository.error?.code === "incompatible_protocol") return <p role="alert">This daemon does not provide compatible repository observability.</p>;
  if (repository.error != null || repository.projection === undefined) return <RetryAlert message={repository.error?.message ?? "Repository evidence is unavailable."} actionLabel={retryLabel} onRetry={() => void repository.retry()} />;

  const projection = repository.projection;

  return (
    <Card asChild>
      <section aria-labelledby="repository-evidence-title">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <CardTitle id="repository-evidence-title">Repository evidence</CardTitle>
          {repository.isRefreshing && <span role="status">Refreshing this panel…</span>}
        </div>
        {repository.isPartial && <p role="alert" className="mt-2 text-warning">Some repository evidence is partial.</p>}
        <dl className="mt-4 grid gap-3 sm:grid-cols-2">
          <Evidence label="Default branch" value={projection.defaultBranch ?? "Unknown"} state={projection.defaultBranchEvidence.state} />
          <Evidence label="Remotes" value={projection.remoteNames.join(", ") || "None recorded"} state={projection.remoteEvidence.state} />
        </dl>
        <h3 className="mt-5 font-semibold">Current and historical locations</h3>
        <ul className="mt-2 space-y-2">
          {projection.displayPaths.map((item) => (
            <li key={`${item.displayPath}:${item.observedFrom}`} className="rounded-lg border border-border p-3">
              <span className="font-mono text-sm">{item.displayPath}</span>
              <span className="ml-2 text-xs uppercase">{item.state}</span>
            </li>
          ))}
        </ul>
        <h3 className="mt-5 font-semibold">Checkouts</h3>
        {projection.checkoutIds.length === 0 ? (
          <p className="mt-2 text-muted-foreground">No checkouts are recorded.</p>
        ) : (
          <ul className="mt-2 flex flex-wrap gap-2">
            {projection.checkoutIds.map((checkoutId) => (
              <li key={checkoutId}>
                <Link to="/workspaces/$workspaceId/checkouts/$checkoutId" params={{ workspaceId, checkoutId }} className={buttonVariants()}>
                  Checkout {checkoutId.slice(0, 8)}
                </Link>
              </li>
            ))}
          </ul>
        )}
      </section>
    </Card>
  );
}

function Evidence({ label, value, state }: { label: string; value: string; state: string }) {
  return (
    <div>
      <dt className="text-xs font-semibold text-muted-foreground uppercase">{label}</dt>
      <dd className="mt-1">{value} <span className="text-xs">({state.replaceAll("_", " ")})</span></dd>
    </div>
  );
}
