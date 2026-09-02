import { Link } from "@tanstack/react-router";

import { Badge } from "../../../components/ui/badge";
import { Card, CardTitle } from "../../../components/ui/card";
import type { WorkspaceId } from "../../../core/contracts";
import { laneOrder, lanePresentations } from "../../board/model/presentation";
import { useFeatureIndex } from "../hooks/useFeatureIndex";
import type { FeatureScope } from "../model/overview";

interface FeatureIndexProps {
  workspaceId: WorkspaceId;
  scope: FeatureScope;
}

export function FeatureIndex({ workspaceId, scope }: FeatureIndexProps) {
  const index = useFeatureIndex(workspaceId, scope);
  if (index.isEmpty) return <Card>No Features are recorded in this scope.</Card>;

  return (
    <div className="space-y-5">
      {index.epics.map((epic) => (
        <Card key={epic.epicId} asChild>
          <section aria-labelledby={`epic-${epic.epicId}`}>
            <CardTitle id={`epic-${epic.epicId}`}>
              <Link to="/workspaces/$workspaceId/epics/$epicId" params={{ workspaceId, epicId: epic.epicId }}>{epic.title}</Link>
            </CardTitle>
            <ul className="mt-3 grid gap-2">
              {epic.features.map((entry) => (
                <li key={entry.feature.id} className="rounded-xl border border-border p-4">
                  <div className="flex flex-wrap items-baseline justify-between gap-3">
                    <Link to="/workspaces/$workspaceId/features/$featureId" params={{ workspaceId, featureId: entry.feature.id }} className="font-medium">{entry.feature.title}</Link>
                    <span className="text-xs text-muted-foreground">{entry.feature.slug} · {entry.workItemCount} Work items</span>
                  </div>
                  {entry.workItemCount > 0 && (
                    <ul className="mt-2 flex flex-wrap gap-2">
                      {laneOrder
                        .filter((status) => (entry.statusCounts[status] ?? 0) > 0)
                        .map((status) => (
                          <li key={status}>
                            <Badge>{lanePresentations[status].title} {entry.statusCounts[status]}</Badge>
                          </li>
                        ))}
                    </ul>
                  )}
                </li>
              ))}
            </ul>
          </section>
        </Card>
      ))}
    </div>
  );
}
