import { Link } from "@tanstack/react-router";

import { NavTabs, navTabVariants } from "../../../components/ui/nav-tabs";
import type { EpicId, WorkItemId, WorkspaceId } from "../../../core/contracts";
import { BoardView } from "../../board/components/BoardView";
import { Breadcrumbs } from "../components/Breadcrumbs";
import { EntityHeader } from "../components/EntityHeader";
import { EntityNotFound } from "../components/EntityNotFound";
import { FeatureIndex } from "../components/FeatureIndex";
import { useEpicOverview } from "../hooks/useEpicOverview";

export type EpicView = "board" | "features";

interface EpicPageProps {
  workspaceId: WorkspaceId;
  epicId: EpicId;
  view: EpicView;
  onOpenWorkItem(workItemId: WorkItemId): void;
}

const viewLabels = {
  board: "Board",
  features: "Features",
} as const satisfies Record<EpicView, string>;

const views = Object.keys(viewLabels) as EpicView[];

export function EpicPage({ workspaceId, epicId, view, onOpenWorkItem }: EpicPageProps) {
  const overview = useEpicOverview(workspaceId, epicId);
  if (overview.isLoading) return <p role="status">Loading Epic…</p>;
  if (overview.isMissing) return <EntityNotFound kind="epic" />;
  if (overview.isUnavailable || overview.hierarchy === undefined) return <p role="alert">The authoritative hierarchy is unavailable.</p>;

  return (
    <div className="space-y-6">
      <Breadcrumbs workspaceId={workspaceId} hierarchy={overview.hierarchy} target={{ kind: "epic", id: epicId }} />
      <EntityHeader
        kind="epic"
        title={overview.epic.title}
        subtitle={`${overview.epic.slug} · ${overview.featureIds.length} Features`}
        repositories={overview.repositories}
      />
      <NavTabs label="Epic views">
        {views.map((value) => (
          <li key={value}>
            <Link
              to="/workspaces/$workspaceId/epics/$epicId"
              params={{ workspaceId, epicId }}
              search={{ view: value }}
              className={navTabVariants({ active: view === value })}
              aria-current={view === value ? "page" : undefined}
            >
              {viewLabels[value]}
            </Link>
          </li>
        ))}
      </NavTabs>
      {view === "board" &&
        (overview.featureIds.length === 0 ? (
          <p>No Features are recorded under this Epic, so it has no board.</p>
        ) : (
          <BoardView workspaceId={workspaceId} scope={{ featureIds: overview.featureIds }} evidenceLinks onOpenWorkItem={onOpenWorkItem} />
        ))}
      {view === "features" && <FeatureIndex workspaceId={workspaceId} scope={{ epicId }} />}
    </div>
  );
}
