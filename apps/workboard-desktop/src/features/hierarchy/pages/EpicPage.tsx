import { Link } from "@tanstack/react-router";

import type { EpicId, WorkItemId, WorkspaceId } from "../../../core/generated";
import { BoardView } from "../../board/components/BoardView";
import { useHierarchy } from "../hooks/useHierarchy";
import { Breadcrumbs } from "../components/Breadcrumbs";
import { EntityHeader } from "../components/EntityHeader";
import { EntityNotFound } from "../components/EntityNotFound";
import { FeatureIndex } from "../components/FeatureIndex";
import { ViewTabs, tabActiveClasses, tabClasses } from "../components/ViewTabs";

export type EpicView = "board" | "features";

interface EpicPageProps {
  workspaceId: WorkspaceId;
  epicId: EpicId;
  view: EpicView;
  onOpenWorkItem(workItemId: WorkItemId): void;
}

const views: Array<[EpicView, string]> = [["board", "Board"], ["features", "Features"]];

export function EpicPage({ workspaceId, epicId, view, onOpenWorkItem }: EpicPageProps) {
  const model = useHierarchy(workspaceId);
  if (model.isLoading) return <p role="status">Loading Epic…</p>;
  if (model.isUnavailable || model.hierarchy === undefined) return <p role="alert">The authoritative hierarchy is unavailable.</p>;
  const entity = model.find("epic", epicId);
  if (entity === undefined) return <EntityNotFound kind="epic" />;
  const featureIds = model.hierarchy.source.features.filter((candidate) => candidate.feature.epicId === epicId).map((candidate) => candidate.feature.id);

  return (
    <div className="space-y-6">
      <Breadcrumbs workspaceId={workspaceId} hierarchy={model.hierarchy.source} target={{ kind: "epic", id: epicId }} />
      <EntityHeader kind="epic" title={entity.title} subtitle={`${entity.subtitle} · ${featureIds.length} Features`} hierarchy={model.hierarchy} repositoryIds={entity.repositoryIds} />
      <ViewTabs label="Epic views">{views.map(([value, label]) => <li key={value}><Link to="/workspaces/$workspaceId/epics/$epicId" params={{ workspaceId, epicId }} search={{ view: value }} className={view === value ? tabActiveClasses : tabClasses} aria-current={view === value ? "page" : undefined}>{label}</Link></li>)}</ViewTabs>
      {view === "board" && (featureIds.length === 0 ? <p>No Features are recorded under this Epic, so it has no board.</p> : <BoardView workspaceId={workspaceId} scope={{ featureIds }} evidenceLinks onOpenWorkItem={onOpenWorkItem} />)}
      {view === "features" && <FeatureIndex workspaceId={workspaceId} hierarchy={model.hierarchy} epicId={epicId} />}
    </div>
  );
}
