import type { InfiniteData, QueryClient } from "@tanstack/react-query";

import type { BoardCardProjection, BoardViewDefinition, EventEnvelope, ReadQueryCode, ResponseEnvelope, WorkspaceId } from "./generated";
import { boardQueryKeys } from "../features/board/api/boardQueryKeys";
import type { BoardResponse } from "../features/board/types/board";
import { checkoutQueryKeys } from "../features/checkout/api/checkoutQueryKeys";
import { hierarchyQueryKeys } from "../features/hierarchy/api/hierarchyQueryKeys";
import { repositoryQueryKeys } from "../features/repository/api/repositoryQueryKeys";
import { savedViewQueryKeys } from "../features/saved-views/api/savedViewQueryKeys";
import { sessionQueryKeys } from "../features/session/api/sessionQueryKeys";
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
  repository_observability: (queryClient, workspaceId) => { void queryClient.invalidateQueries({ queryKey: repositoryQueryKeys.workspace(workspaceId) }); },
  checkout_observability: (queryClient, workspaceId) => { void queryClient.invalidateQueries({ queryKey: checkoutQueryKeys.workspace(workspaceId) }); },
  session_observability: (queryClient, workspaceId) => { void queryClient.invalidateQueries({ queryKey: sessionQueryKeys.all(workspaceId) }); },
  recovery_preview: (queryClient, workspaceId) => { void queryClient.invalidateQueries({ queryKey: ["recovery-previews", workspaceId] }); },
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

function patchBoardCards(queryClient: QueryClient, workspaceId: WorkspaceId, sequence: number, changedCards: BoardCardProjection[]) {
  const cardsByWorkItem = new Map(changedCards.map((card) => [card.workItem.id, card]));
  queryClient.setQueriesData<InfiniteData<BoardResponse>>({ queryKey: boardQueryKeys.boards(workspaceId) }, (current) => {
    if (current === undefined) return current;
    let changed = false;
    const pages = current.pages.map((page) => {
      if (page.result?.type !== "board") return page;
      let pageChanged = false;
      const cards = page.result.value.cards.map((candidate) => {
        const replacement = cardsByWorkItem.get(candidate.workItem.id);
        if (replacement === undefined) return candidate;
        changed = true;
        pageChanged = true;
        return replacement;
      });
      return pageChanged ? { ...page, authoritativeRevision: Math.max(page.authoritativeRevision ?? 0, sequence), result: { type: "board" as const, value: { ...page.result.value, cards } } } : page;
    });
    return changed ? { ...current, pages } : current;
  });
}

export function applyWorkspaceEvent(queryClient: QueryClient, event: EventEnvelope) {
  if (event.payload?.type === "board_view_saved") {
    patchSavedView(queryClient, event.workspaceId, event.sequence, event.payload.value.view);
  }
  if (event.payload?.type === "board_card_changed") {
    patchBoardCards(queryClient, event.workspaceId, event.sequence, [event.payload.value.card]);
  }
  if (event.payload?.type === "checkout_changed") {
    const { checkout, cards } = event.payload.value;
    queryClient.setQueryData<ResponseEnvelope>(checkoutQueryKeys.detail(event.workspaceId, checkout.id), (current) => current === undefined ? current : { ...current, authoritativeRevision: Math.max(current.authoritativeRevision ?? 0, event.sequence), result: { type: "checkout_observability", value: checkout } });
    void queryClient.invalidateQueries({ queryKey: repositoryQueryKeys.detail(event.workspaceId, checkout.repository.id) });
    patchBoardCards(queryClient, event.workspaceId, event.sequence, cards);
  }
  if (event.payload?.type === "session_liveness_changed") {
    const { session, recovery, cards } = event.payload.value;
    queryClient.setQueryData<ResponseEnvelope>(sessionQueryKeys.detail(event.workspaceId, session.id), (current) => current === undefined ? current : { ...current, authoritativeRevision: Math.max(current.authoritativeRevision ?? 0, event.sequence), result: { type: "session_observability", value: session } });
    queryClient.setQueryData<ResponseEnvelope>(sessionQueryKeys.recovery(event.workspaceId, session.id), (current) => current === undefined ? current : { ...current, authoritativeRevision: Math.max(current.authoritativeRevision ?? 0, event.sequence), result: { type: "recovery_preview", value: recovery } });
    patchBoardCards(queryClient, event.workspaceId, event.sequence, cards);
  }
  for (const query of event.invalidationScope?.queries ?? []) {
    if (event.payload?.type === "board_view_saved" && (query === "board_views" || query === "board_view")) continue;
    if (event.payload?.type === "board_card_changed" && query === "board") continue;
    if (event.payload?.type === "checkout_changed" && (query === "checkout_observability" || query === "repository_observability" || query === "board")) continue;
    if (event.payload?.type === "session_liveness_changed" && (query === "session_observability" || query === "recovery_preview" || query === "board")) continue;
    readQueryInvalidators[query](queryClient, event.workspaceId);
  }
}
