import type { InfiniteData, QueryClient } from "@tanstack/react-query";

import type { BoardViewDefinition, EventEnvelope, ReadQueryCode, ResponseEnvelope, WorkspaceId } from "./generated";
import { boardQueryKeys } from "../features/board/api/boardQueryKeys";
import type { BoardResponse } from "../features/board/types/board";
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
  board: (queryClient, workspaceId) => { void queryClient.invalidateQueries({ queryKey: boardQueryKeys.boards(workspaceId) }); },
  attention: (queryClient, workspaceId) => { void queryClient.invalidateQueries({ queryKey: boardQueryKeys.attention(workspaceId) }); },
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

function patchBoardCard(queryClient: QueryClient, event: EventEnvelope & { payload: Extract<NonNullable<EventEnvelope["payload"]>, { type: "board_card_changed" }> }) {
  const card = event.payload.value.card;
  queryClient.setQueriesData<InfiniteData<BoardResponse>>({ queryKey: boardQueryKeys.boards(event.workspaceId) }, (current) => {
    if (current === undefined) return current;
    let changed = false;
    const pages = current.pages.map((page) => {
      if (page.result?.type !== "board") return page;
      let pageChanged = false;
      const cards = page.result.value.cards.map((candidate) => {
        if (candidate.workItem.id !== card.workItem.id) return candidate;
        changed = true;
        pageChanged = true;
        return card;
      });
      return pageChanged ? { ...page, authoritativeRevision: Math.max(page.authoritativeRevision ?? 0, event.sequence), result: { type: "board" as const, value: { ...page.result.value, cards } } } : page;
    });
    return changed ? { ...current, pages } : current;
  });
}

export function applyWorkspaceEvent(queryClient: QueryClient, event: EventEnvelope) {
  if (event.payload?.type === "board_view_saved") {
    patchSavedView(queryClient, event.workspaceId, event.sequence, event.payload.value.view);
  }
  if (event.payload?.type === "board_card_changed") {
    patchBoardCard(queryClient, event as EventEnvelope & { payload: Extract<NonNullable<EventEnvelope["payload"]>, { type: "board_card_changed" }> });
  }
  for (const query of event.invalidationScope?.queries ?? []) {
    if (event.payload?.type === "board_view_saved" && (query === "board_views" || query === "board_view")) continue;
    if (event.payload?.type === "board_card_changed" && query === "board") continue;
    readQueryInvalidators[query](queryClient, event.workspaceId);
  }
}
