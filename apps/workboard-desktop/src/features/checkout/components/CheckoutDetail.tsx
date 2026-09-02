import { Link } from "@tanstack/react-router";

import { buttonVariants } from "../../../components/ui/button";
import { Card, CardTitle } from "../../../components/ui/card";
import { RetryAlert } from "../../../components/ui/retry-alert";
import type { CheckoutAvailability, CheckoutId, EvidenceState, WorkspaceId } from "../../../core/contracts";
import { useCheckout } from "../hooks/useCheckout";

const availabilityLabels: Record<CheckoutAvailability, string> = { available: "Available", missing: "Missing", deleted: "Deleted", replaced: "Replaced" };
const evidenceLabels: Record<EvidenceState, string> = { current: "Current", historical: "Historical", stale: "Stale", missing: "Missing", unknown: "Unknown", conflict: "Conflict", not_loaded: "Not loaded" };

const retryLabel = "Retry checkout panel";

export function CheckoutDetail({ workspaceId, checkoutId }: { workspaceId: WorkspaceId; checkoutId: CheckoutId }) {
  const checkout = useCheckout(workspaceId, checkoutId);
  if (checkout.isLoading) return <p role="status">Loading checkout evidence…</p>;
  if (checkout.isDisconnected) return <RetryAlert message="Checkout evidence is disconnected." actionLabel={retryLabel} onRetry={() => void checkout.retry()} />;
  if (checkout.error?.code === "projection_version_unavailable" || checkout.error?.code === "incompatible_protocol") return <p role="alert">This daemon does not provide compatible checkout observability.</p>;
  if (checkout.error != null || checkout.projection === undefined) return <RetryAlert message={checkout.error?.message ?? "Checkout evidence is unavailable."} actionLabel={retryLabel} onRetry={() => void checkout.retry()} />;

  const projection = checkout.projection;

  return (
    <div className="space-y-5">
      <header>
        <p className="text-sm text-muted-foreground">{projection.repository.title}</p>
        <h1 className="text-2xl font-semibold">Checkout {checkoutId.slice(0, 8)}</h1>
        <p className="mt-1">Status: {availabilityLabels[projection.availability]}</p>
      </header>
      {checkout.isRefreshing && <p role="status">Refreshing this checkout…</p>}
      {checkout.isPartial && <p role="alert">Some checkout evidence is partial.</p>}

      <Card asChild>
        <section aria-labelledby="checkout-identity">
          <CardTitle id="checkout-identity">Identity and purpose</CardTitle>
          <dl className="mt-3 grid gap-3 sm:grid-cols-2">
            <Item label="Purpose" value={projection.purpose.replaceAll("_", " ")} />
            <Item label="Purpose source" value={projection.purposeSource.replaceAll("_", " ")} />
            <Item label="Branch" value={projection.branch ?? "Unknown"} />
            <Item label="Head" value={projection.head ?? "Unknown"} />
            <Item label="Isolation generation" value={projection.isolationGeneration?.toString() ?? "Not loaded"} />
            <Item label="Reconciliation generation" value={projection.reconciliationGeneration?.toString() ?? "Not loaded"} />
          </dl>
        </section>
      </Card>

      <Card asChild>
        <section aria-labelledby="checkout-evidence">
          <CardTitle id="checkout-evidence">Operational evidence</CardTitle>
          <ul className="mt-3 grid gap-3 md:grid-cols-3">
            {[projection.dirtyEvidence, projection.collisionEvidence, projection.reconciliationEvidence].map((item) => (
              <li key={item.code} className="rounded-lg border border-border p-3">
                <strong>{evidenceLabels[item.state]}</strong>
                <p className="mt-1 text-sm">{item.message}</p>
              </li>
            ))}
          </ul>
        </section>
      </Card>

      <section aria-labelledby="checkout-locations">
        <h2 id="checkout-locations" className="font-semibold">Current and historical locations</h2>
        <ul className="mt-2 space-y-2">
          {projection.displayPaths.map((item) => (
            <li key={`${item.displayPath}:${item.observedFrom}`} className="rounded-lg border border-border p-3">
              <span className="font-mono text-sm">{item.displayPath}</span> · {evidenceLabels[item.state]}
            </li>
          ))}
        </ul>
      </section>

      <section aria-labelledby="checkout-sessions">
        <h2 id="checkout-sessions" className="font-semibold">Bound sessions</h2>
        {projection.sessionIds.length === 0 ? (
          <p className="mt-2">No bound sessions.</p>
        ) : (
          <ul className="mt-2 flex flex-wrap gap-2">
            {projection.sessionIds.map((sessionId) => (
              <li key={sessionId}>
                <Link to="/workspaces/$workspaceId/sessions/$sessionId" params={{ workspaceId, sessionId }} className={buttonVariants()}>
                  Session {sessionId.slice(0, 8)}
                </Link>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section aria-labelledby="checkout-work-items">
        <h2 id="checkout-work-items" className="font-semibold">Work items</h2>
        {projection.bindings.every((binding) => binding.workItemId == null) ? (
          <p className="mt-2">No Work-item binding is recorded.</p>
        ) : (
          <ul className="mt-2 flex flex-wrap gap-2">
            {projection.bindings
              .flatMap((binding) => (binding.workItemId == null ? [] : [binding.workItemId]))
              .map((workItemId) => (
                <li key={workItemId}>
                  <Link to="/workspaces/$workspaceId/work-items/$workItemId" params={{ workspaceId, workItemId }} className={buttonVariants()}>
                    Work item {workItemId.slice(0, 8)}
                  </Link>
                </li>
              ))}
          </ul>
        )}
      </section>
    </div>
  );
}

function Item({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt className="text-xs font-semibold text-muted-foreground uppercase">{label}</dt>
      <dd className="mt-1 break-all">{value}</dd>
    </div>
  );
}
