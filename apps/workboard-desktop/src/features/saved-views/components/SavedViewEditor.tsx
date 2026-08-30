import { useState } from "react";
import type { ZodIssue } from "zod";

import type { BoardViewDefinition, BoardViewDensity, BoardViewGroupingKind, BoardViewSortDirection, BoardViewSortField, WorkItemStatus, WorkspaceId } from "../../../core/generated";
import { useHierarchy } from "../../hierarchy/hooks/useHierarchy";
import { useSavedViewEditor } from "../hooks/useSavedViewEditor";

const groupingLabels: Record<BoardViewGroupingKind, string> = { hierarchy: "Hierarchy", repository: "Repository", status: "Status" };
const densityLabels: Record<BoardViewDensity, string> = { comfortable: "Comfortable", compact: "Compact" };
const sortFieldLabels: Record<BoardViewSortField, string> = { title: "Title", key: "Work-item key" };
const sortDirectionLabels: Record<BoardViewSortDirection, string> = { ascending: "Ascending", descending: "Descending" };
const statusLabels: Record<WorkItemStatus, string> = { backlog: "Backlog", ready: "Ready", in_progress: "In progress", blocked: "Blocked", review: "Review", done: "Done", cancelled: "Cancelled" };

export function SavedViewEditor({ workspaceId, view }: { workspaceId: WorkspaceId; view?: BoardViewDefinition }) {
  const editor = useSavedViewEditor(workspaceId, view);
  const hierarchy = useHierarchy(workspaceId);
  const [issues, setIssues] = useState<ZodIssue[]>([]);
  if (editor.draft === undefined) return <button type="button" onClick={editor.begin} className="rounded-lg bg-[var(--accent)] px-4 py-2 font-medium text-[var(--accent-contrast)]">{view === undefined ? "Create service view" : "Edit view"}</button>;
  const fieldIssue = (field: string) => issues.find((issue) => issue.path.join(".").endsWith(field))?.message;

  return (
    <form onSubmit={(event) => { event.preventDefault(); const parsed = editor.submit(); setIssues(parsed.success ? [] : parsed.error.issues); }} className="grid gap-5" aria-label="Saved view editor">
      <label className="grid gap-1"><span>Title</span><input value={editor.draft.title} onChange={(event) => editor.setTitle(event.target.value)} className="rounded-lg border border-[var(--border)] bg-[var(--canvas)] px-3 py-2" />{fieldIssue("title") && <span role="alert" className="text-sm text-[var(--warning)]">{fieldIssue("title")}</span>}</label>
      <label className="grid gap-1"><span>Hierarchy search</span><input value={editor.draft.query} onChange={(event) => editor.setQuery(event.target.value)} className="rounded-lg border border-[var(--border)] bg-[var(--canvas)] px-3 py-2" /></label>
      <fieldset><legend className="font-medium">Repository/service filters</legend><div className="mt-2 grid gap-2 sm:grid-cols-2">{hierarchy.hierarchy?.repositories.map((repository) => <label key={repository.id} className="flex items-center gap-2"><input type="checkbox" checked={editor.draft?.repositoryIds.includes(repository.id)} onChange={() => editor.toggleRepository(repository.id)} />{repository.title}</label>)}</div></fieldset>
      <fieldset><legend className="font-medium">Work-item status filters</legend><div className="mt-2 flex flex-wrap gap-3">{Object.entries(statusLabels).map(([status, label]) => <label key={status} className="flex items-center gap-2"><input type="checkbox" checked={editor.draft?.statuses.includes(status as WorkItemStatus)} onChange={() => editor.toggleStatus(status as WorkItemStatus)} />{label}</label>)}</div></fieldset>
      <div className="grid gap-4 sm:grid-cols-2"><label className="grid gap-1"><span>Grouping</span><select value={editor.draft.groupingKind} onChange={(event) => editor.setGroupingKind(event.target.value as BoardViewGroupingKind)}>{Object.entries(groupingLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label><label className="grid gap-1"><span>Density</span><select value={editor.draft.density} onChange={(event) => editor.setDensity(event.target.value as BoardViewDensity)}>{Object.entries(densityLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label><label className="grid gap-1"><span>Sort by</span><select value={editor.draft.sortField} onChange={(event) => editor.setSortField(event.target.value as BoardViewSortField)}>{Object.entries(sortFieldLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label><label className="grid gap-1"><span>Sort direction</span><select value={editor.draft.sortDirection} onChange={(event) => editor.setSortDirection(event.target.value as BoardViewSortDirection)}>{Object.entries(sortDirectionLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label></div>
      {!editor.canSave && <p role="status" className="rounded-lg border border-[var(--warning-muted)] p-3 text-[var(--warning)]">Read-only: {editor.readOnlyReason} Your unsaved view remains available in this window.</p>}
      <div className="flex gap-3"><button type="submit" disabled={!editor.canSave || editor.isSaving} className="rounded-lg bg-[var(--accent)] px-4 py-2 font-medium text-[var(--accent-contrast)] disabled:opacity-50">{editor.isSaving ? "Saving…" : "Save view"}</button><button type="button" onClick={editor.cancel} className="rounded-lg border border-[var(--border)] px-4 py-2">Cancel</button></div>
    </form>
  );
}
