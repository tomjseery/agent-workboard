import { Link } from "@tanstack/react-router";

import { Badge } from "../../../components/ui/badge";
import { Card, CardTitle } from "../../../components/ui/card";
import type { FeatureId, WorkspaceId } from "../../../core/contracts";
import { laneOrder, lanePresentations } from "../../board/model/presentation";
import { useFeatureOverview } from "../hooks/useFeatureOverview";

interface FeatureDetailProps {
  workspaceId: WorkspaceId;
  featureId: FeatureId;
}

export function FeatureDetail({ workspaceId, featureId }: FeatureDetailProps) {
  const overview = useFeatureOverview(workspaceId, featureId);
  if (overview.isMissing || overview.hierarchy === undefined) return null;
  const { workItems, statusCounts } = overview;

  return (
    <div className="space-y-5">
      <Card asChild>
        <section aria-labelledby="feature-rollup-title">
          <CardTitle id="feature-rollup-title">Work-item status</CardTitle>
          {workItems.length === 0 ? (
            <p className="mt-2">No Work items are recorded for this Feature.</p>
          ) : (
            <dl className="mt-4 grid grid-cols-2 gap-3 lg:grid-cols-4">
              {laneOrder.map((status) => (
                <div key={status} className="rounded-xl bg-muted p-4">
                  <dt className="text-sm text-muted-foreground">{lanePresentations[status].title}</dt>
                  <dd className="mt-1 text-2xl font-semibold">{statusCounts[status] ?? 0}</dd>
                </div>
              ))}
            </dl>
          )}
        </section>
      </Card>
      <Card asChild>
        <section aria-labelledby="feature-items-title">
          <CardTitle id="feature-items-title">Work items</CardTitle>
          {workItems.length === 0 ? (
            <p className="mt-2">No Work items are recorded for this Feature.</p>
          ) : (
            <ul className="mt-3 grid gap-2">
              {workItems.map((entry) => (
                <li key={entry.workItem.id} className="flex flex-wrap items-baseline justify-between gap-3 rounded-xl border border-border p-3">
                  <Link to="/workspaces/$workspaceId/work-items/$workItemId" params={{ workspaceId, workItemId: entry.workItem.id }}>
                    <span className="font-mono text-xs text-primary">{entry.workItem.key}</span> <span className="font-medium">{entry.workItem.title}</span>
                  </Link>
                  <Badge>{lanePresentations[entry.status].title}</Badge>
                </li>
              ))}
            </ul>
          )}
        </section>
      </Card>
    </div>
  );
}
