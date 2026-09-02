import { Link } from "@tanstack/react-router";

import type { RepositoryId, WorkItemId, WorkspaceId } from "../../../core/generated";
import { BoardView } from "../../board/components/BoardView";
import { RepositoryObservabilityPanel } from "../../repository/components/RepositoryObservabilityPanel";
import { useHierarchy } from "../hooks/useHierarchy";
import { Breadcrumbs } from "../components/Breadcrumbs";
import { EntityHeader } from "../components/EntityHeader";
import { EntityNotFound } from "../components/EntityNotFound";
import { FeatureIndex } from "../components/FeatureIndex";
import { ViewTabs, tabActiveClasses, tabClasses } from "../components/ViewTabs";

export type RepositoryView = "board" | "features" | "evidence";

interface RepositoryPageProps {
  workspaceId: WorkspaceId;
  repositoryId: RepositoryId;
  view: RepositoryView;
  onOpenWorkItem(workItemId: WorkItemId): void;
}

const views: Array<[RepositoryView, string]> = [["board", "Board"], ["features", "Features"], ["evidence", "Repository evidence"]];

export function RepositoryPage({ workspaceId, repositoryId, view, onOpenWorkItem }: RepositoryPageProps) {
  const model = useHierarchy(workspaceId);
  if (model.isLoading) return <p role="status">Loading repository…</p>;
  if (model.isUnavailable || model.hierarchy === undefined) return <p role="alert">The authoritative hierarchy is unavailable.</p>;
  const entity = model.find("repository", repositoryId);
  if (entity === undefined) return <EntityNotFound kind="repository" />;
  const features = model.hierarchy.source.features.filter((candidate) => candidate.repositoryIds.includes(repositoryId));

  return (
    <div className="space-y-6">
      <Breadcrumbs workspaceId={workspaceId} hierarchy={model.hierarchy.source} target={{ kind: "repository", id: repositoryId }} />
      <EntityHeader kind="repository" title={entity.title} subtitle={`${entity.subtitle} · ${features.length} Features`} hierarchy={model.hierarchy} repositoryIds={entity.repositoryIds} />
      <ViewTabs label="Repository views">{views.map(([value, label]) => <li key={value}><Link to="/workspaces/$workspaceId/repositories/$repositoryId" params={{ workspaceId, repositoryId }} search={{ view: value }} className={view === value ? tabActiveClasses : tabClasses} aria-current={view === value ? "page" : undefined}>{label}</Link></li>)}</ViewTabs>
      {view === "board" && <BoardView workspaceId={workspaceId} scope={{ repositoryIds: [repositoryId] }} evidenceLinks onOpenWorkItem={onOpenWorkItem} />}
      {view === "features" && <FeatureIndex workspaceId={workspaceId} hierarchy={model.hierarchy} repositoryId={repositoryId} />}
      {view === "evidence" && <RepositoryObservabilityPanel workspaceId={workspaceId} repositoryId={repositoryId} />}
    </div>
  );
}
