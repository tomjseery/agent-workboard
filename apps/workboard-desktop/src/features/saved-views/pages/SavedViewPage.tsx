import { Card, CardEyebrow } from "../../../components/ui/card";
import type { BoardViewId, WorkItemId, WorkspaceId } from "../../../core/contracts";
import { BoardView } from "../../board/components/BoardView";
import { SavedViewEditor } from "../components/SavedViewEditor";
import { useSavedView } from "../hooks/useSavedView";

interface SavedViewPageProps {
  workspaceId: WorkspaceId;
  viewId: BoardViewId;
  onOpenWorkItem(workItemId: WorkItemId): void;
}

export function SavedViewPage({ workspaceId, viewId, onOpenWorkItem }: SavedViewPageProps) {
  const state = useSavedView(workspaceId, viewId);
  if (state.isLoading) return <p role="status">Loading saved view…</p>;
  if (state.isMissing || state.view === undefined) return <section><h1 className="text-2xl font-semibold">Saved view not found</h1><p className="mt-2">This deep link is missing or incompatible with the current daemon.</p></section>;
  return (
    <div className="space-y-6">
      <Card asChild className="p-6">
      <header>
        <CardEyebrow>Saved service view</CardEyebrow>
        <h1 className="mt-2 text-3xl font-semibold">{state.view.title}</h1>
        <p className="mt-2 text-sm text-muted-foreground">Revision {state.view.revision} · {state.view.grouping.kind} grouping · {state.view.density} density</p>
        <div className="mt-5"><SavedViewEditor workspaceId={workspaceId} view={state.view} /></div>
      </header>
      </Card>
      <BoardView workspaceId={workspaceId} scope={{ repositoryIds: state.view.filters.repositoryIds }} evidenceLinks onOpenWorkItem={onOpenWorkItem} />
    </div>
  );
}
