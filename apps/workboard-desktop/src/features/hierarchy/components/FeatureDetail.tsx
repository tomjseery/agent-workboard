import { Link } from "@tanstack/react-router";

import type { FeatureId, WorkItemStatus, WorkspaceId } from "../../../core/generated";
import { laneOrder, lanePresentations } from "../../board/types/presentation";
import type { HierarchyModel } from "../types/hierarchy";

interface FeatureDetailProps {
  workspaceId: WorkspaceId;
  hierarchy: HierarchyModel;
  featureId: FeatureId;
}

export function FeatureDetail({ workspaceId, hierarchy, featureId }: FeatureDetailProps) {
  const workItems = hierarchy.source.workItems.filter((entry) => entry.workItem.featureId === featureId);
  const counts = new Map<WorkItemStatus, number>();
  for (const item of workItems) counts.set(item.status, (counts.get(item.status) ?? 0) + 1);

  return (
    <div className="space-y-5">
      <section aria-labelledby="feature-rollup-title" className="rounded-2xl border border-[var(--border)] bg-[var(--surface)] p-5">
        <h2 id="feature-rollup-title" className="text-lg font-semibold">Work-item status</h2>
        {workItems.length === 0 ? <p className="mt-2">No Work items are recorded for this Feature.</p> : <dl className="mt-4 grid grid-cols-2 gap-3 lg:grid-cols-4">{laneOrder.map((status) => <div key={status} className="rounded-xl bg-[var(--surface-muted)] p-4"><dt className="text-sm text-[var(--muted-text)]">{lanePresentations[status].title}</dt><dd className="mt-1 text-2xl font-semibold">{counts.get(status) ?? 0}</dd></div>)}</dl>}
      </section>
      <section aria-labelledby="feature-items-title" className="rounded-2xl border border-[var(--border)] bg-[var(--surface)] p-5">
        <h2 id="feature-items-title" className="text-lg font-semibold">Work items</h2>
        {workItems.length === 0 ? <p className="mt-2">No Work items are recorded for this Feature.</p> : <ul className="mt-3 grid gap-2">{[...workItems].sort((left, right) => left.workItem.key.localeCompare(right.workItem.key)).map((entry) => <li key={entry.workItem.id} className="flex flex-wrap items-baseline justify-between gap-3 rounded-xl border border-[var(--border)] p-3"><Link to="/workspaces/$workspaceId/work-items/$workItemId" params={{ workspaceId, workItemId: entry.workItem.id }}><span className="font-mono text-xs text-[var(--accent)]">{entry.workItem.key}</span> <span className="font-medium">{entry.workItem.title}</span></Link><span className="rounded-full border border-[var(--border)] px-2.5 py-0.5 text-xs">{lanePresentations[entry.status].title}</span></li>)}</ul>}
      </section>
    </div>
  );
}
