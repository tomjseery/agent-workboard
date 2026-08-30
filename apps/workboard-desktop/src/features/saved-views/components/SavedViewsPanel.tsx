import type { WorkspaceId } from "../../../core/generated";
import { SavedViewEditor } from "./SavedViewEditor";
import { SavedViewsList } from "./SavedViewsList";
import { useSavedViews } from "../hooks/useSavedViews";

export function SavedViewsPanel({ workspaceId }: { workspaceId: WorkspaceId }) {
  const state = useSavedViews(workspaceId);
  return <section aria-labelledby="saved-views-title" className="rounded-2xl border border-[var(--border)] bg-[var(--surface)] p-5"><div className="flex flex-wrap items-center justify-between gap-4"><div><h2 id="saved-views-title" className="text-lg font-semibold">Saved service views</h2><p className="text-sm text-[var(--muted-text)]">Filters remain views over this Workspace.</p></div><SavedViewEditor workspaceId={workspaceId} /></div>{!state.canSave && state.readOnlyReason !== undefined && <p role="status" className="mt-4 rounded-lg border border-[var(--warning-muted)] p-3 text-[var(--warning)]">Read-only: {state.readOnlyReason}</p>}<div className="mt-4"><SavedViewsList workspaceId={workspaceId} /></div></section>;
}
