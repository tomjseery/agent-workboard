import { Alert } from "../../../components/ui/alert";
import { Card, CardTitle } from "../../../components/ui/card";
import type { WorkspaceId } from "../../../core/contracts";
import { useSavedViews } from "../hooks/useSavedViews";
import { SavedViewEditor } from "./SavedViewEditor";
import { SavedViewsList } from "./SavedViewsList";

export function SavedViewsPanel({ workspaceId }: { workspaceId: WorkspaceId }) {
  const state = useSavedViews(workspaceId);
  return (
    <Card asChild>
      <section aria-labelledby="saved-views-title">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div>
            <CardTitle id="saved-views-title">Saved service views</CardTitle>
            <p className="text-sm text-muted-foreground">Filters remain views over this Workspace.</p>
          </div>
          <SavedViewEditor workspaceId={workspaceId} />
        </div>
        {!state.canSave && state.readOnlyReason !== undefined && (
          <Alert role="status" className="mt-4 text-warning">Read-only: {state.readOnlyReason}</Alert>
        )}
        <div className="mt-4">
          <SavedViewsList workspaceId={workspaceId} />
        </div>
      </section>
    </Card>
  );
}
