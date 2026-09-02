import { Link } from "@tanstack/react-router";

import { NavTabs, navTabVariants } from "../../../components/ui/nav-tabs";
import type { RepositoryId, WorkItemId, WorkspaceId } from "../../../core/contracts";
import { BoardView } from "../../board/components/BoardView";
import { RepositoryObservabilityPanel } from "../../repository/components/RepositoryObservabilityPanel";
import { Breadcrumbs } from "../components/Breadcrumbs";
import { EntityHeader } from "../components/EntityHeader";
import { EntityNotFound } from "../components/EntityNotFound";
import { FeatureIndex } from "../components/FeatureIndex";
import { useRepositoryOverview } from "../hooks/useRepositoryOverview";

export type RepositoryView = "board" | "features" | "evidence";

interface RepositoryPageProps {
  workspaceId: WorkspaceId;
  repositoryId: RepositoryId;
  view: RepositoryView;
  onOpenWorkItem(workItemId: WorkItemId): void;
}

const viewLabels = {
  board: "Board",
  features: "Features",
  evidence: "Repository evidence",
} as const satisfies Record<RepositoryView, string>;

const views = Object.keys(viewLabels) as RepositoryView[];

export function RepositoryPage({ workspaceId, repositoryId, view, onOpenWorkItem }: RepositoryPageProps) {
  const overview = useRepositoryOverview(workspaceId, repositoryId);
  if (overview.isLoading) return <p role="status">Loading repository…</p>;
  if (overview.isMissing) return <EntityNotFound kind="repository" />;
  if (overview.isUnavailable || overview.hierarchy === undefined) return <p role="alert">The authoritative hierarchy is unavailable.</p>;

  return (
    <div className="space-y-6">
      <Breadcrumbs workspaceId={workspaceId} hierarchy={overview.hierarchy} target={{ kind: "repository", id: repositoryId }} />
      <EntityHeader
        kind="repository"
        title={overview.repository.title}
        subtitle={`${overview.repository.slug} · ${overview.featureCount} Features`}
        repositories={overview.repositories}
      />
      <NavTabs label="Repository views">
        {views.map((value) => (
          <li key={value}>
            <Link
              to="/workspaces/$workspaceId/repositories/$repositoryId"
              params={{ workspaceId, repositoryId }}
              search={{ view: value }}
              className={navTabVariants({ active: view === value })}
              aria-current={view === value ? "page" : undefined}
            >
              {viewLabels[value]}
            </Link>
          </li>
        ))}
      </NavTabs>
      {view === "board" && <BoardView workspaceId={workspaceId} scope={{ repositoryIds: [repositoryId] }} evidenceLinks onOpenWorkItem={onOpenWorkItem} />}
      {view === "features" && <FeatureIndex workspaceId={workspaceId} scope={{ repositoryId }} />}
      {view === "evidence" && <RepositoryObservabilityPanel workspaceId={workspaceId} repositoryId={repositoryId} />}
    </div>
  );
}
