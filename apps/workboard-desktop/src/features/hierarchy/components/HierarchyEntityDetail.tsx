import type { WorkspaceId } from "../../../core/generated";
import { useHierarchy } from "../hooks/useHierarchy";
import type { HierarchyEntityKind } from "../types/hierarchy";
import { Breadcrumbs } from "./Breadcrumbs";
import { HierarchyNavigation } from "./HierarchyNavigation";
import { RepositoryParticipation } from "./RepositoryParticipation";

interface HierarchyEntityDetailProps {
  workspaceId: WorkspaceId;
  kind: HierarchyEntityKind;
  entityId: string;
  query: string;
  onQueryChange(query: string): void;
}

interface EntityPresentation {
  eyebrow: string;
  participation: string;
}

export const entityPresentations: Record<HierarchyEntityKind, EntityPresentation> = {
  repository: { eyebrow: "Repository", participation: "Workspace participation" },
  epic: { eyebrow: "Epic", participation: "Participating repositories" },
  feature: { eyebrow: "Feature", participation: "Cross-repository scope" },
  work_item: { eyebrow: "Work item", participation: "Repository scope" },
};

export function HierarchyEntityDetail({ workspaceId, kind, entityId, query, onQueryChange }: HierarchyEntityDetailProps) {
  const model = useHierarchy(workspaceId);
  if (model.isLoading) return <p role="status">Loading {entityPresentations[kind].eyebrow}…</p>;
  if (model.isUnavailable || model.hierarchy === undefined) return <p role="alert">The authoritative hierarchy is unavailable.</p>;
  const entity = model.find(kind, entityId);
  if (entity === undefined) return <section aria-labelledby="missing-title"><h1 id="missing-title">{entityPresentations[kind].eyebrow} not found</h1><p>This deep link no longer resolves in the current Workspace hierarchy.</p></section>;

  return (
    <div className="space-y-6">
      <Breadcrumbs workspaceId={workspaceId} hierarchy={model.hierarchy} entity={entity} />
      <header className="rounded-2xl border border-[var(--border)] bg-[var(--surface)] p-6">
        <p className="text-xs font-semibold uppercase tracking-[0.18em] text-[var(--accent)]">{entityPresentations[kind].eyebrow}</p>
        <h1 className="mt-2 text-3xl font-semibold">{entity.title}</h1>
        <p className="mt-1 text-[var(--muted-text)]">{entity.subtitle}</p>
        <h2 className="mt-5 text-sm font-semibold">{entityPresentations[kind].participation}</h2>
        <div className="mt-2"><RepositoryParticipation hierarchy={model.hierarchy} repositoryIds={entity.repositoryIds} /></div>
      </header>
      <HierarchyNavigation workspaceId={workspaceId} query={query} repositoryIds={kind === "repository" ? [entity.id] : undefined} onQueryChange={onQueryChange} />
    </div>
  );
}
