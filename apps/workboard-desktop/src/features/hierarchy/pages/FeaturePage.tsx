import { Link } from "@tanstack/react-router";

import type { FeatureId, WorkItemId, WorkspaceId } from "../../../core/generated";
import { BoardView } from "../../board/components/BoardView";
import { ProposalDetail } from "../../proposal/components/ProposalDetail";
import { useHierarchy } from "../hooks/useHierarchy";
import { Breadcrumbs } from "../components/Breadcrumbs";
import { EntityHeader } from "../components/EntityHeader";
import { EntityNotFound } from "../components/EntityNotFound";
import { FeatureDetail } from "../components/FeatureDetail";
import { ViewTabs, tabActiveClasses, tabClasses } from "../components/ViewTabs";

export type FeatureTab = "board" | "detail" | "proposal";

interface FeaturePageProps {
  workspaceId: WorkspaceId;
  featureId: FeatureId;
  tab: FeatureTab;
  onOpenWorkItem(workItemId: WorkItemId): void;
}

const tabs: Array<[FeatureTab, string]> = [["board", "Board"], ["detail", "Detail"], ["proposal", "Proposal"]];

export function FeaturePage({ workspaceId, featureId, tab, onOpenWorkItem }: FeaturePageProps) {
  const model = useHierarchy(workspaceId);
  if (model.isLoading) return <p role="status">Loading Feature…</p>;
  if (model.isUnavailable || model.hierarchy === undefined) return <p role="alert">The authoritative hierarchy is unavailable.</p>;
  const entity = model.find("feature", featureId);
  if (entity === undefined) return <EntityNotFound kind="feature" />;
  const workItems = model.hierarchy.source.workItems.filter((candidate) => candidate.workItem.featureId === featureId);

  return (
    <div className="space-y-6">
      <Breadcrumbs workspaceId={workspaceId} hierarchy={model.hierarchy.source} target={{ kind: "feature", id: featureId }} />
      <EntityHeader kind="feature" title={entity.title} subtitle={`${entity.subtitle} · ${workItems.length} Work items`} hierarchy={model.hierarchy} repositoryIds={entity.repositoryIds} />
      <ViewTabs label="Feature views">{tabs.map(([value, label]) => <li key={value}><Link to="/workspaces/$workspaceId/features/$featureId" params={{ workspaceId, featureId }} search={{ tab: value }} className={tab === value ? tabActiveClasses : tabClasses} aria-current={tab === value ? "page" : undefined}>{label}</Link></li>)}</ViewTabs>
      {tab === "board" && <BoardView workspaceId={workspaceId} scope={{ featureIds: [featureId] }} evidenceLinks onOpenWorkItem={onOpenWorkItem} />}
      {tab === "detail" && <FeatureDetail workspaceId={workspaceId} hierarchy={model.hierarchy} featureId={featureId} />}
      {tab === "proposal" && <ProposalDetail workspaceId={workspaceId} featureId={featureId} />}
    </div>
  );
}
