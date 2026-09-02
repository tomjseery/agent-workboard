import { Link } from "@tanstack/react-router";

import { NavTabs, navTabVariants } from "../../../components/ui/nav-tabs";
import type { FeatureId, WorkItemId, WorkspaceId } from "../../../core/contracts";
import { BoardView } from "../../board/components/BoardView";
import { ProposalDetail } from "../../proposal/components/ProposalDetail";
import { Breadcrumbs } from "../components/Breadcrumbs";
import { EntityHeader } from "../components/EntityHeader";
import { EntityNotFound } from "../components/EntityNotFound";
import { FeatureDetail } from "../components/FeatureDetail";
import { useFeatureOverview } from "../hooks/useFeatureOverview";

export type FeatureTab = "board" | "detail" | "proposal";

interface FeaturePageProps {
  workspaceId: WorkspaceId;
  featureId: FeatureId;
  tab: FeatureTab;
  onOpenWorkItem(workItemId: WorkItemId): void;
}

const tabLabels = {
  board: "Board",
  detail: "Detail",
  proposal: "Proposal",
} as const satisfies Record<FeatureTab, string>;

const tabs = Object.keys(tabLabels) as FeatureTab[];

export function FeaturePage({ workspaceId, featureId, tab, onOpenWorkItem }: FeaturePageProps) {
  const overview = useFeatureOverview(workspaceId, featureId);
  if (overview.isLoading) return <p role="status">Loading Feature…</p>;
  if (overview.isMissing) return <EntityNotFound kind="feature" />;
  if (overview.isUnavailable || overview.hierarchy === undefined) return <p role="alert">The authoritative hierarchy is unavailable.</p>;

  return (
    <div className="space-y-6">
      <Breadcrumbs workspaceId={workspaceId} hierarchy={overview.hierarchy} target={{ kind: "feature", id: featureId }} />
      <EntityHeader
        kind="feature"
        title={overview.feature.title}
        subtitle={`${overview.feature.slug} · ${overview.workItems.length} Work items`}
        repositories={overview.repositories}
      />
      <NavTabs label="Feature views">
        {tabs.map((value) => (
          <li key={value}>
            <Link
              to="/workspaces/$workspaceId/features/$featureId"
              params={{ workspaceId, featureId }}
              search={{ tab: value }}
              className={navTabVariants({ active: tab === value })}
              aria-current={tab === value ? "page" : undefined}
            >
              {tabLabels[value]}
            </Link>
          </li>
        ))}
      </NavTabs>
      {tab === "board" && <BoardView workspaceId={workspaceId} scope={{ featureIds: [featureId] }} evidenceLinks onOpenWorkItem={onOpenWorkItem} />}
      {tab === "detail" && <FeatureDetail workspaceId={workspaceId} featureId={featureId} />}
      {tab === "proposal" && <ProposalDetail workspaceId={workspaceId} featureId={featureId} />}
    </div>
  );
}
