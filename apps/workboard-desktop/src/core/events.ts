import type { QueryClient } from "@tanstack/react-query";

import type { BoardViewDefinition, EventEnvelope, ReadQueryCode, ResponseEnvelope, WorkspaceId } from "./generated";
import { hierarchyQueryKeys } from "../features/hierarchy/api/hierarchyQueryKeys";
import { savedViewQueryKeys } from "../features/saved-views/api/savedViewQueryKeys";
import { workspaceQueryKeys } from "../features/workspace/api/workspaceQueryKeys";

type Invalidator = (queryClient: QueryClient, workspaceId: WorkspaceId) => void;

export const readQueryInvalidators: Record<ReadQueryCode, Invalidator> = {
  workspace_summary: (queryClient, workspaceId) => { void queryClient.invalidateQueries({ queryKey: workspaceQueryKeys.detail(workspaceId) }); },
  hierarchy_children: () => undefined,
  workspace_hierarchy: (queryClient, workspaceId) => { void queryClient.invalidateQueries({ queryKey: hierarchyQueryKeys.workspace(workspaceId) }); },
  board_views: (queryClient, workspaceId) => { void queryClient.invalidateQueries({ queryKey: savedViewQueryKeys.list(workspaceId) }); },
  board_view: (queryClient, workspaceId) => { void queryClient.invalidateQueries({ queryKey: savedViewQueryKeys.details(workspaceId) }); },
  board_snapshot: () => undefined,
};

function patchSavedView(queryClient: QueryClient, workspaceId: WorkspaceId, sequence: number, view: BoardViewDefinition) {
  queryClient.setQueryData<ResponseEnvelope>(savedViewQueryKeys.detail(workspaceId, view.id), (current) => current === undefined ? current : { ...current, authoritativeRevision: Math.max(current.authoritativeRevision ?? 0, sequence), result: { type: "board_view", value: view } });
  queryClient.setQueryData<ResponseEnvelope>(savedViewQueryKeys.list(workspaceId), (current) => {
    if (current?.result?.type !== "board_views") return current;
    const views = current.result.value.filter((candidate) => candidate.id !== view.id);
    views.push(view);
    views.sort((left, right) => left.title.localeCompare(right.title));
    return { ...current, authoritativeRevision: Math.max(current.authoritativeRevision ?? 0, sequence), result: { type: "board_views", value: views } };
  });
}

export function applyWorkspaceEvent(queryClient: QueryClient, event: EventEnvelope) {
  if (event.payload?.type === "board_view_saved") {
    patchSavedView(queryClient, event.workspaceId, event.sequence, event.payload.value.view);
  }
  for (const query of event.invalidationScope?.queries ?? []) {
    if (event.payload?.type === "board_view_saved" && (query === "board_views" || query === "board_view")) continue;
    readQueryInvalidators[query](queryClient, event.workspaceId);
  }
}
