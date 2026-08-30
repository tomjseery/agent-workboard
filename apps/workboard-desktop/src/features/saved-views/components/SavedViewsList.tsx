import { Link } from "@tanstack/react-router";

import type { WorkspaceId } from "../../../core/generated";
import { useSavedViews } from "../hooks/useSavedViews";

export function SavedViewsList({ workspaceId }: { workspaceId: WorkspaceId }) {
  const state = useSavedViews(workspaceId);
  if (state.isLoading) return <p role="status">Loading saved views…</p>;
  if (state.isUnavailable) return <p role="alert">Saved views are unavailable. The unsaved hierarchy remains usable.</p>;
  if (state.views.length === 0) return <p className="text-sm text-[var(--muted-text)]">No saved service views yet.</p>;
  return <ul className="grid gap-2">{state.views.map((view) => <li key={view.id}><Link to="/workspaces/$workspaceId/views/$viewId" params={{ workspaceId, viewId: view.id }} search={{ q: "" }} className="block rounded-lg border border-[var(--border)] px-3 py-2">{view.title}</Link></li>)}</ul>;
}
