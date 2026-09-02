import { useId, useState } from "react";
import type { ZodIssue } from "zod";

import { Alert } from "../../../components/ui/alert";
import { Button } from "../../../components/ui/button";
import { Checkbox } from "../../../components/ui/checkbox";
import { Input } from "../../../components/ui/input";
import { Label } from "../../../components/ui/label";
import { Select } from "../../../components/ui/select";
import type { BoardViewDefinition, BoardViewDensity, BoardViewGroupingKind, BoardViewSortDirection, BoardViewSortField, WorkItemStatus, WorkspaceId } from "../../../core/contracts";
import { useHierarchy } from "../../hierarchy/hooks/useHierarchy";
import { useSavedViewEditor } from "../hooks/useSavedViewEditor";

const groupingLabels: Record<BoardViewGroupingKind, string> = { hierarchy: "Hierarchy", repository: "Repository", status: "Status" };
const densityLabels: Record<BoardViewDensity, string> = { comfortable: "Comfortable", compact: "Compact" };
const sortFieldLabels: Record<BoardViewSortField, string> = { title: "Title", key: "Work-item key" };
const sortDirectionLabels: Record<BoardViewSortDirection, string> = { ascending: "Ascending", descending: "Descending" };
const statusLabels: Record<WorkItemStatus, string> = { backlog: "Backlog", ready: "Ready", in_progress: "In progress", blocked: "Blocked", review: "Review", done: "Done", cancelled: "Cancelled" };

export function SavedViewEditor({ workspaceId, view }: { workspaceId: WorkspaceId; view?: BoardViewDefinition }) {
  const editor = useSavedViewEditor(workspaceId, view);
  const { hierarchy } = useHierarchy(workspaceId);
  const [issues, setIssues] = useState<ZodIssue[]>([]);
  const fieldId = useId();

  if (editor.draft === undefined) {
    return <Button type="button" variant="solid" size="lg" onClick={editor.begin}>{view === undefined ? "Create service view" : "Edit view"}</Button>;
  }

  const draft = editor.draft;
  const fieldIssue = (field: string) => issues.find((issue) => issue.path.join(".").endsWith(field))?.message;

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        const parsed = editor.submit();
        setIssues(parsed.success ? [] : parsed.error.issues);
      }}
      className="grid gap-5"
      aria-label="Saved view editor"
    >
      <div className="grid gap-1">
        <Label htmlFor={`${fieldId}-title`}>Title</Label>
        <Input id={`${fieldId}-title`} value={draft.title} onChange={(event) => editor.setTitle(event.target.value)} />
        {fieldIssue("title") && <span role="alert" className="text-sm text-warning">{fieldIssue("title")}</span>}
      </div>
      <div className="grid gap-1">
        <Label htmlFor={`${fieldId}-query`}>Hierarchy search</Label>
        <Input id={`${fieldId}-query`} value={draft.query} onChange={(event) => editor.setQuery(event.target.value)} />
      </div>
      <fieldset>
        <legend className="font-medium">Repository/service filters</legend>
        <div className="mt-2 grid gap-2 sm:grid-cols-2">
          {hierarchy?.repositories.map((repository) => (
            <div key={repository.id} className="flex items-center gap-2">
              <Checkbox
                id={`${fieldId}-repository-${repository.id}`}
                checked={draft.repositoryIds.includes(repository.id)}
                onCheckedChange={() => editor.toggleRepository(repository.id)}
              />
              <Label htmlFor={`${fieldId}-repository-${repository.id}`}>{repository.title}</Label>
            </div>
          ))}
        </div>
      </fieldset>
      <fieldset>
        <legend className="font-medium">Work-item status filters</legend>
        <div className="mt-2 flex flex-wrap gap-3">
          {Object.entries(statusLabels).map(([status, label]) => (
            <div key={status} className="flex items-center gap-2">
              <Checkbox
                id={`${fieldId}-status-${status}`}
                checked={draft.statuses.includes(status as WorkItemStatus)}
                onCheckedChange={() => editor.toggleStatus(status as WorkItemStatus)}
              />
              <Label htmlFor={`${fieldId}-status-${status}`}>{label}</Label>
            </div>
          ))}
        </div>
      </fieldset>
      <div className="grid gap-4 sm:grid-cols-2">
        <div className="grid gap-1">
          <Label htmlFor={`${fieldId}-grouping`}>Grouping</Label>
          <Select id={`${fieldId}-grouping`} value={draft.groupingKind} onChange={(event) => editor.setGroupingKind(event.target.value as BoardViewGroupingKind)}>
            {Object.entries(groupingLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}
          </Select>
        </div>
        <div className="grid gap-1">
          <Label htmlFor={`${fieldId}-density`}>Density</Label>
          <Select id={`${fieldId}-density`} value={draft.density} onChange={(event) => editor.setDensity(event.target.value as BoardViewDensity)}>
            {Object.entries(densityLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}
          </Select>
        </div>
        <div className="grid gap-1">
          <Label htmlFor={`${fieldId}-sort-field`}>Sort by</Label>
          <Select id={`${fieldId}-sort-field`} value={draft.sortField} onChange={(event) => editor.setSortField(event.target.value as BoardViewSortField)}>
            {Object.entries(sortFieldLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}
          </Select>
        </div>
        <div className="grid gap-1">
          <Label htmlFor={`${fieldId}-sort-direction`}>Sort direction</Label>
          <Select id={`${fieldId}-sort-direction`} value={draft.sortDirection} onChange={(event) => editor.setSortDirection(event.target.value as BoardViewSortDirection)}>
            {Object.entries(sortDirectionLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}
          </Select>
        </div>
      </div>
      {!editor.canSave && (
        <Alert role="status" className="text-warning">Read-only: {editor.readOnlyReason} Your unsaved view remains available in this window.</Alert>
      )}
      <div className="flex gap-3">
        <Button type="submit" variant="solid" size="lg" disabled={!editor.canSave || editor.isSaving}>
          {editor.isSaving ? "Saving…" : "Save view"}
        </Button>
        <Button type="button" size="lg" onClick={editor.cancel}>Cancel</Button>
      </div>
    </form>
  );
}
