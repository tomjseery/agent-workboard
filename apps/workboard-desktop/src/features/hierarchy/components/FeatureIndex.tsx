import { Link } from "@tanstack/react-router";

import type { EpicId, RepositoryId, WorkItemStatus, WorkspaceId } from "../../../core/generated";
import { laneOrder, lanePresentations } from "../../board/types/presentation";
import type { HierarchyModel } from "../types/hierarchy";

interface FeatureIndexProps {
  workspaceId: WorkspaceId;
  hierarchy: HierarchyModel;
  repositoryId?: RepositoryId;
  epicId?: EpicId;
}

export function FeatureIndex({ workspaceId, hierarchy, repositoryId, epicId }: FeatureIndexProps) {
  const source = hierarchy.source;
  const features = source.features
    .filter((entry) => (repositoryId === undefined || entry.repositoryIds.includes(repositoryId)) && (epicId === undefined || entry.feature.epicId === epicId))
    .sort((left, right) => left.feature.title.localeCompare(right.feature.title));
  if (features.length === 0) return <p className="rounded-2xl border border-[var(--border)] bg-[var(--surface)] p-5">No Features are recorded in this scope.</p>;
  const epics = [...new Set(features.map((entry) => entry.feature.epicId))]
    .map((id) => ({ id, title: source.epics.find((entry) => entry.epic.id === id)?.epic.title ?? "No Epic" }))
    .sort((left, right) => left.title.localeCompare(right.title));

  return (
    <div className="space-y-5">
      {epics.map((epic) => (
        <section key={epic.id} aria-labelledby={`epic-${epic.id}`} className="rounded-2xl border border-[var(--border)] bg-[var(--surface)] p-5">
          <h2 id={`epic-${epic.id}`} className="text-lg font-semibold"><Link to="/workspaces/$workspaceId/epics/$epicId" params={{ workspaceId, epicId: epic.id }}>{epic.title}</Link></h2>
          <ul className="mt-3 grid gap-2">
            {features.filter((entry) => entry.feature.epicId === epic.id).map((entry) => {
              const items = source.workItems.filter((item) => item.workItem.featureId === entry.feature.id && (repositoryId === undefined || item.repositoryIds.includes(repositoryId)));
              const counts = new Map<WorkItemStatus, number>();
              for (const item of items) counts.set(item.status, (counts.get(item.status) ?? 0) + 1);
              return (
                <li key={entry.feature.id} className="rounded-xl border border-[var(--border)] p-4">
                  <div className="flex flex-wrap items-baseline justify-between gap-3">
                    <Link to="/workspaces/$workspaceId/features/$featureId" params={{ workspaceId, featureId: entry.feature.id }} className="font-medium">{entry.feature.title}</Link>
                    <span className="text-xs text-[var(--muted-text)]">{entry.feature.slug} · {items.length} Work items</span>
                  </div>
                  {items.length > 0 && <ul className="mt-2 flex flex-wrap gap-2">{laneOrder.filter((status) => (counts.get(status) ?? 0) > 0).map((status) => <li key={status} className="rounded-full border border-[var(--border)] px-2.5 py-0.5 text-xs">{lanePresentations[status].title} {counts.get(status)}</li>)}</ul>}
                </li>
              );
            })}
          </ul>
        </section>
      ))}
    </div>
  );
}
