import type { BoardViewDefinition, WorkspaceId } from "../../../core/contracts";
import { boardViewDefinitionSchema } from "../schemas/boardViewDefinitionSchema";
import { useSavedViewDraftStore } from "../store/savedViewDraftStore";
import { useSaveBoardViewMutation } from "./useSaveBoardViewMutation";
import { useSavedViews } from "./useSavedViews";

export function useSavedViewEditor(workspaceId: WorkspaceId, source?: BoardViewDefinition) {
  const savedViews = useSavedViews(workspaceId);
  const draft = useSavedViewDraftStore((state) => state.draft);
  const begin = useSavedViewDraftStore((state) => state.begin);
  const setTitle = useSavedViewDraftStore((state) => state.setTitle);
  const setQuery = useSavedViewDraftStore((state) => state.setQuery);
  const toggleRepository = useSavedViewDraftStore((state) => state.toggleRepository);
  const setGroupingKind = useSavedViewDraftStore((state) => state.setGroupingKind);
  const setDensity = useSavedViewDraftStore((state) => state.setDensity);
  const setSortField = useSavedViewDraftStore((state) => state.setSortField);
  const setSortDirection = useSavedViewDraftStore((state) => state.setSortDirection);
  const toggleStatus = useSavedViewDraftStore((state) => state.toggleStatus);
  const clear = useSavedViewDraftStore((state) => state.clear);
  const mutation = useSaveBoardViewMutation(workspaceId, savedViews.workspaceRevision);
  const activeDraft = draft?.workspaceId === workspaceId && (source === undefined || draft.id === source.id) ? draft : undefined;

  const submit = () => {
    const parsed = boardViewDefinitionSchema.safeParse(activeDraft === undefined ? undefined : buildBoardViewDefinition(activeDraft));
    if (parsed.success && savedViews.canSave) {
      mutation.mutate(parsed.data, { onSuccess: clear });
    }
    return parsed;
  };

  return {
    draft: activeDraft,
    begin: () => begin(workspaceId, source),
    cancel: clear,
    setTitle,
    setQuery,
    toggleRepository,
    setGroupingKind,
    setDensity,
    setSortField,
    setSortDirection,
    toggleStatus,
    submit,
    canSave: savedViews.canSave,
    readOnlyReason: savedViews.readOnlyReason,
    isSaving: mutation.isPending,
  };
}

export function buildBoardViewDefinition(draft: NonNullable<ReturnType<typeof useSavedViewDraftStore.getState>["draft"]>) {
  return {
    id: draft.id,
    workspaceId: draft.workspaceId,
    title: draft.title,
    filters: { query: draft.query.trim() === "" ? null : draft.query, repositoryIds: draft.repositoryIds, statuses: draft.statuses },
    grouping: { kind: draft.groupingKind, lanes: draft.lanes },
    sort: { field: draft.sortField, direction: draft.sortDirection },
    density: draft.density,
    revision: draft.revision,
  };
}
