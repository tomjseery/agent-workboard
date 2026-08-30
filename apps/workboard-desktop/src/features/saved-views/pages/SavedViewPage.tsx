import type { BoardViewId, WorkspaceId } from "../../../core/generated";
import { HierarchyNavigation } from "../../hierarchy/components/HierarchyNavigation";
import { SavedViewEditor } from "../components/SavedViewEditor";
import { useSavedView } from "../hooks/useSavedView";

export function SavedViewPage({ workspaceId, viewId, query, onQueryChange }: { workspaceId: WorkspaceId; viewId: BoardViewId; query: string; onQueryChange(query: string): void }) {
  const state = useSavedView(workspaceId, viewId);
  if (state.isLoading) return <p role="status">Loading saved view…</p>;
  if (state.isMissing || state.view === undefined) return <section><h1>Saved view not found</h1><p>This deep link is missing or incompatible with the current daemon.</p></section>;
  const effectiveQuery = query.trim() === "" ? state.view.filters.query ?? "" : query;
  return <div className="space-y-6"><header className="rounded-2xl border border-[var(--border)] bg-[var(--surface)] p-6"><p className="text-xs font-semibold uppercase tracking-[0.18em] text-[var(--accent)]">Saved service view</p><h1 className="mt-2 text-3xl font-semibold">{state.view.title}</h1><p className="mt-2 text-sm text-[var(--muted-text)]">Revision {state.view.revision} · {state.view.grouping.kind} grouping · {state.view.density} density</p><div className="mt-5"><SavedViewEditor workspaceId={workspaceId} view={state.view} /></div></header><HierarchyNavigation workspaceId={workspaceId} query={effectiveQuery} repositoryIds={state.view.filters.repositoryIds} statuses={state.view.filters.statuses} sort={state.view.sort} density={state.view.density} onQueryChange={onQueryChange} /></div>;
}
