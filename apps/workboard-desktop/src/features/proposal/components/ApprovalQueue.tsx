import { Link } from "@tanstack/react-router";

import { Alert } from "../../../components/ui/alert";
import { Badge } from "../../../components/ui/badge";
import { Card, CardEyebrow } from "../../../components/ui/card";
import { RetryAlert } from "../../../components/ui/retry-alert";
import type { WorkspaceId } from "../../../core/contracts";
import { useApprovalQueue } from "../hooks/useProposal";

const retryLabel = "Retry approval queue";

export function ApprovalQueue({ workspaceId }: { workspaceId: WorkspaceId }) {
  const model = useApprovalQueue(workspaceId);
  if (model.isLoading) return <p role="status">Loading approval queue…</p>;
  if (model.isDisconnected) return <RetryAlert message="The approval queue is disconnected." actionLabel={retryLabel} onRetry={() => void model.retry()} />;
  if (model.error?.code === "projection_version_unavailable" || model.error?.code === "incompatible_protocol") return <p role="alert">This daemon does not provide a compatible approval queue. No local queue has been inferred.</p>;
  if (model.error != null || model.projection === undefined) return <RetryAlert message={model.error?.message ?? "The approval queue is unavailable."} actionLabel={retryLabel} onRetry={() => void model.retry()} />;
  if (model.projection.entries.length === 0) return <p>No Feature proposals currently require review.</p>;

  return (
    <section aria-labelledby="approval-queue-title" className="space-y-4">
      <header>
        <CardEyebrow>Daemon-owned ordering</CardEyebrow>
        <h1 id="approval-queue-title" className="mt-1 text-3xl font-semibold">Feature proposal reviews</h1>
      </header>
      {model.isRefreshing && <p role="status">Refreshing the approval queue…</p>}
      {model.isStale && <p role="status">The queue may be stale while Workboard refreshes.</p>}
      {model.partialOutcomes.map((outcome) => <Alert key={outcome.code}>{outcome.message}</Alert>)}
      <ol className="grid gap-3">
        {model.projection.entries.map((entry) => (
          <li key={entry.feature.id} aria-posinset={entry.position} aria-setsize={entry.totalCount}>
            <Card size="compact">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div>
                  <h2 className="font-semibold">
                    <Link to="/workspaces/$workspaceId/features/$featureId/proposal" params={{ workspaceId, featureId: entry.feature.id }} className="underline decoration-border underline-offset-4">
                      {entry.feature.title}
                    </Link>
                  </h2>
                  <p className="text-sm text-muted-foreground">Generation {entry.generation} · revision {entry.revision}</p>
                </div>
                <Badge size="lg">{entry.workflowState.replaceAll("_", " ")}</Badge>
              </div>
              {entry.changedSincePrevious && <p role="alert" className="mt-3 text-warning">Changed proposal — review the new hash.</p>}
              <p className="mt-3 text-sm">{entry.repositories.map((repository) => repository.slug).join(", ") || "No repository scope"}</p>
              <p className="mt-1 text-xs text-muted-foreground">{entry.warningCount} warnings · {entry.plannerCount} planners</p>
              <p className="mt-1 font-mono text-xs break-all">{entry.proposalHash}</p>
            </Card>
          </li>
        ))}
      </ol>
    </section>
  );
}
