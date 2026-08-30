import type { BoardViewDensity, BoardViewSort, EntityRef, RepositoryId, WorkItemStatus, WorkspaceId } from "../../../core/generated";
import { useHierarchy } from "../hooks/useHierarchy";
import type { HierarchyEntityModel } from "../types/hierarchy";
import { EntityLink } from "./EntityLink";
import { RepositoryParticipation } from "./RepositoryParticipation";

interface HierarchyNavigationProps {
  workspaceId: WorkspaceId;
  query: string;
  repositoryIds?: RepositoryId[];
  statuses?: WorkItemStatus[];
  sort?: BoardViewSort;
  density?: BoardViewDensity;
  onQueryChange?(query: string): void;
}

const refKinds: Record<EntityRef["kind"], (id: string, entities: HierarchyEntityModel[]) => HierarchyEntityModel | undefined> = {
  workspace: () => undefined,
  repository: (id, entities) => entities.find((entity) => entity.kind === "repository" && entity.id === id),
  epic: (id, entities) => entities.find((entity) => entity.kind === "epic" && entity.id === id),
  feature: (id, entities) => entities.find((entity) => entity.kind === "feature" && entity.id === id),
  work_item: (id, entities) => entities.find((entity) => entity.kind === "work_item" && entity.id === id),
  session: () => undefined,
};

export function HierarchyNavigation({ workspaceId, query, repositoryIds, statuses, sort, density = "comfortable", onQueryChange }: HierarchyNavigationProps) {
  const model = useHierarchy(workspaceId, query, repositoryIds, statuses, sort);
  if (model.isLoading) return <p role="status">Loading hierarchy…</p>;
  if (model.isUnavailable || model.hierarchy === undefined) return <p role="alert">The authoritative hierarchy is unavailable.</p>;
  const hierarchy = model.hierarchy;
  const all = [...hierarchy.repositories, ...hierarchy.epics, ...hierarchy.features, ...hierarchy.workItems];
  const recent = hierarchy.source.recentEntities.map((entity) => refKinds[entity.kind](entity.id, all)).filter((entity): entity is HierarchyEntityModel => entity !== undefined);
  const focusedRef = hierarchy.source.focusedEntity;
  const focused = focusedRef === null ? undefined : refKinds[focusedRef.kind](focusedRef.id, all);

  return (
    <section aria-labelledby="hierarchy-title" className="rounded-2xl border border-[var(--border)] bg-[var(--surface)] p-5">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div><h2 id="hierarchy-title" className="text-lg font-semibold">Hierarchy</h2><p className="text-sm text-[var(--muted-text)]">Search across repositories, Epics, Features, and Work items.</p></div>
        <label className="grid gap-1 text-sm"><span>Search hierarchy</span><input autoFocus value={query} onChange={(event) => onQueryChange?.(event.target.value)} className="w-72 max-w-full rounded-lg border border-[var(--border)] bg-[var(--canvas)] px-3 py-2" /></label>
      </div>
      {focused !== undefined && <aside aria-label="Focused entity" className="mt-5 rounded-xl bg-[var(--accent-surface)] p-4"><p className="text-xs font-semibold uppercase tracking-wider text-[var(--accent)]">Focused</p><EntityLink entity={focused} workspaceId={workspaceId} /></aside>}
      {recent.length > 0 && <nav aria-label="Recent entities" className="mt-5"><h3 className="text-sm font-semibold">Recent</h3><ul className="mt-2 flex flex-wrap gap-2">{recent.map((entity) => <li key={`${entity.kind}-${entity.id}`}><EntityLink entity={entity} workspaceId={workspaceId} /></li>)}</ul></nav>}
      {model.visible.length === 0 ? <p className="mt-6 text-[var(--muted-text)]">No hierarchy entities match this view.</p> : <ul aria-label="Hierarchy results" className={`mt-6 grid ${density === "compact" ? "gap-1" : "gap-3"}`}>{model.visible.map((entity) => <li key={`${entity.kind}-${entity.id}`} className={`rounded-xl border border-[var(--border)] ${density === "compact" ? "p-2" : "p-4"}`}><div className="flex flex-wrap justify-between gap-3"><div><p className="text-xs uppercase tracking-wider text-[var(--muted-text)]">{entity.kind.replace("_", " ")}</p><p className="font-medium"><EntityLink entity={entity} workspaceId={workspaceId} /></p><p className="text-sm text-[var(--muted-text)]">{entity.subtitle}</p></div><RepositoryParticipation hierarchy={hierarchy} repositoryIds={entity.repositoryIds} /></div></li>)}</ul>}
    </section>
  );
}
