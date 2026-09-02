import { QueryClient } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";

import type { BoardViewDefinition, EventEnvelope, ResponseEnvelope } from "./generated";
import current from "./generated/conformance-current.json";
import { applyWorkspaceEvent } from "./events";
import { checkoutQueryKeys } from "../features/checkout/api/checkoutQueryKeys";
import { hierarchyQueryKeys } from "../features/hierarchy/api/hierarchyQueryKeys";
import { savedViewQueryKeys } from "../features/saved-views/api/savedViewQueryKeys";
import { workspaceQueryKeys } from "../features/workspace/api/workspaceQueryKeys";
import { boardQueryKeys } from "../features/board/api/boardQueryKeys";
import { repositoryQueryKeys } from "../features/repository/api/repositoryQueryKeys";
import { sessionQueryKeys } from "../features/session/api/sessionQueryKeys";
import { proposalQueryKeys } from "../features/proposal/api/proposalQueryKeys";
import { createLargeBoardFixture } from "../features/board/fixtures/largeBoardFixture";
import { workItemQueryKeys } from "../features/work-item/api/workItemQueryKeys";

const workspaceId = "20000000-0000-0000-0000-000000000001";
const viewId = "a0000000-0000-0000-0000-000000000001";
const requestId = "10000000-0000-0000-0000-000000000001";
const view: BoardViewDefinition = { id: viewId, workspaceId, title: "Service", filters: { query: null, repositoryIds: [], statuses: [] }, grouping: { kind: "hierarchy", lanes: [] }, sort: { field: "title", direction: "ascending" }, density: "comfortable", revision: 2 };

function envelope(result: ResponseEnvelope["result"]): ResponseEnvelope {
  return { protocolVersion: 5, requestId, correlationId: requestId, workspaceId, authoritativeRevision: 1, serverTimestamp: "2026-08-30T12:00:00Z", result, error: null, diagnostics: [], availableActions: [], partialOutcomes: [] };
}

function event(queries: EventEnvelope["invalidationScope"] extends infer Scope ? Scope extends { queries: infer Queries } ? Queries : never : never): EventEnvelope {
  return { protocolVersion: 5, eventVersion: 1, workspaceId, sequence: 2, eventId: "90000000-0000-0000-0000-000000000001", occurredAt: "2026-08-30T12:00:01Z", owner: { kind: "workspace", id: workspaceId }, entityRevision: 2, kind: "board_view_saved", payload: { type: "board_view_saved", value: { view } }, invalidationScope: { queries, owners: [{ kind: "workspace", id: workspaceId }] }, operationCorrelationId: requestId, partialOutcomes: [] };
}

describe("ordered event cache updates", () => {
  it("patches only canonical saved-view queries for a saved-view event", () => {
    const queryClient = new QueryClient();
    const workspace = envelope({ type: "workspace_summary", value: { workspace: { id: workspaceId, slug: "workspace", title: "Workspace" }, repositoryCount: 0, epicCount: 0, featureCount: 0, workItemCount: 0, sessionCount: 0 } });
    queryClient.setQueryData(workspaceQueryKeys.detail(workspaceId), workspace);
    queryClient.setQueryData(savedViewQueryKeys.list(workspaceId), envelope({ type: "board_views", value: [] }));
    queryClient.setQueryData(savedViewQueryKeys.detail(workspaceId, viewId), envelope(null));
    applyWorkspaceEvent(queryClient, event(["board_views", "board_view"]));
    expect(queryClient.getQueryData(workspaceQueryKeys.detail(workspaceId))).toBe(workspace);
    expect((queryClient.getQueryData<ResponseEnvelope>(savedViewQueryKeys.list(workspaceId))?.result as { value: BoardViewDefinition[] }).value).toEqual([view]);
    expect(queryClient.getQueryData<ResponseEnvelope>(savedViewQueryKeys.detail(workspaceId, viewId))?.result).toEqual({ type: "board_view", value: view });
  });

  it("invalidates hierarchy without touching Workspace summary", () => {
    const queryClient = new QueryClient();
    queryClient.setQueryData(workspaceQueryKeys.detail(workspaceId), envelope(null));
    queryClient.setQueryData(hierarchyQueryKeys.workspace(workspaceId), envelope(null));
    const hierarchyEvent = { ...event(["workspace_hierarchy"]), kind: "projection_changed" as const, payload: { type: "projection_changed" as const, value: { entity: { kind: "feature" as const, id: "50000000-0000-0000-0000-000000000001" } } } };
    applyWorkspaceEvent(queryClient, hierarchyEvent);
    expect(queryClient.getQueryState(hierarchyQueryKeys.workspace(workspaceId))?.isInvalidated).toBe(true);
    expect(queryClient.getQueryState(workspaceQueryKeys.detail(workspaceId))?.isInvalidated).toBe(false);
  });

  it("patches one card while preserving unrelated card and canonical query references", () => {
    const queryClient = new QueryClient();
    const fixture = createLargeBoardFixture();
    const parameters = { limit: 200, query: null, repositoryIds: [], featureIds: [], statuses: [], laneKeys: [], sort: { field: "key" as const, direction: "ascending" as const } };
    const first = fixture.cards[0]!;
    const untouched = fixture.cards[1]!;
    const boardEnvelope = envelope({ type: "board", value: { lanes: fixture.lanes, cards: [first, untouched], nextCursor: null, totalCount: 2, revision: 1 } });
    const boardKey = boardQueryKeys.board(workspaceId, parameters);
    const unrelatedKey = boardQueryKeys.board(workspaceId, { ...parameters, statuses: ["done"] });
    const attentionKey = boardQueryKeys.attentionList(workspaceId, { limit: 200, repositoryIds: [], reasonCodes: [] });
    const unrelatedEnvelope = envelope({ type: "board", value: { lanes: fixture.lanes, cards: [untouched], nextCursor: null, totalCount: 1, revision: 1 } });
    const unrelated = { pages: [unrelatedEnvelope], pageParams: [null] };
    queryClient.setQueryData(boardKey, { pages: [boardEnvelope], pageParams: [null] });
    queryClient.setQueryData(unrelatedKey, unrelated);
    queryClient.setQueryData(attentionKey, { pages: [envelope({ type: "attention", value: { entries: fixture.attentionEntries.slice(0, 1), nextCursor: null, totalCount: fixture.attentionEntries.length, revision: 1 } })], pageParams: [null] });
    const changed = { ...first, revision: first.revision + 1, workItem: { ...first.workItem, title: "Changed title" } };
    const changedEvent = { ...event(["board", "attention"]), kind: "projection_changed" as const, payload: { type: "board_card_changed" as const, value: { card: changed } }, owner: { kind: "work_item" as const, id: first.workItem.id } };
    applyWorkspaceEvent(queryClient, changedEvent);
    const updated = queryClient.getQueryData<{ pages: ResponseEnvelope[] }>(boardKey)!;
    const result = updated.pages[0]!.result;
    expect(result?.type).toBe("board");
    if (result?.type !== "board") throw new Error("board projection missing");
    expect(result.value.cards[0]!.workItem.title).toBe("Changed title");
    expect(result.value.cards[1]).toBe(untouched);
    expect(queryClient.getQueryData(unrelatedKey)).toBe(unrelated);
    expect(queryClient.getQueryState(boardKey)?.isInvalidated).toBe(false);
    expect(queryClient.getQueryState(attentionKey)?.isInvalidated).toBe(true);
  });

  it("patches only an affected checkout repository and canonical cards", () => {
    const queryClient = new QueryClient();
    const payload = current.discriminants.eventPayloads.find(({ type }) => type === "checkout_changed") as unknown as Extract<NonNullable<EventEnvelope["payload"]>, { type: "checkout_changed" }>;
    const checkout = payload.value.checkout;
    const unrelatedRepositoryId = "30000000-0000-0000-0000-000000000099";
    const unrelatedCheckoutId = "b0000000-0000-0000-0000-000000000099";
    const affectedRepository = envelope(null);
    const unrelatedRepository = envelope(null);
    const unrelatedCheckout = envelope(null);
    queryClient.setQueryData(checkoutQueryKeys.detail(workspaceId, checkout.id), envelope(null));
    queryClient.setQueryData(checkoutQueryKeys.detail(workspaceId, unrelatedCheckoutId), unrelatedCheckout);
    queryClient.setQueryData(repositoryQueryKeys.detail(workspaceId, checkout.repository.id), affectedRepository);
    queryClient.setQueryData(repositoryQueryKeys.detail(workspaceId, unrelatedRepositoryId), unrelatedRepository);
    const fixture = createLargeBoardFixture();
    const parameters = { limit: 200, query: null, repositoryIds: [], featureIds: [], statuses: [], laneKeys: [], sort: { field: "key" as const, direction: "ascending" as const } };
    const first = fixture.cards[0]!;
    const untouched = fixture.cards[1]!;
    const boardKey = boardQueryKeys.board(workspaceId, parameters);
    queryClient.setQueryData(boardKey, { pages: [envelope({ type: "board", value: { lanes: fixture.lanes, cards: [first, untouched], nextCursor: null, totalCount: 2, revision: 1 } })], pageParams: [null] });
    applyWorkspaceEvent(queryClient, { ...event(["checkout_observability", "repository_observability", "board"]), kind: "checkout_changed", payload, owner: { kind: "repository", id: checkout.repository.id } });
    expect(queryClient.getQueryData<ResponseEnvelope>(checkoutQueryKeys.detail(workspaceId, checkout.id))?.result).toEqual({ type: "checkout_observability", value: checkout });
    expect(queryClient.getQueryData(checkoutQueryKeys.detail(workspaceId, unrelatedCheckoutId))).toBe(unrelatedCheckout);
    expect(queryClient.getQueryState(repositoryQueryKeys.detail(workspaceId, checkout.repository.id))?.isInvalidated).toBe(true);
    expect(queryClient.getQueryData(repositoryQueryKeys.detail(workspaceId, unrelatedRepositoryId))).toBe(unrelatedRepository);
    const result = queryClient.getQueryData<{ pages: ResponseEnvelope[] }>(boardKey)!.pages[0]!.result;
    if (result?.type !== "board") throw new Error("board projection missing");
    expect(result.value.cards[0]).toEqual(payload.value.cards[0]);
    expect(result.value.cards[1]).toBe(untouched);
  });

  it("patches one session and its recovery preview without touching unrelated panels", () => {
    const queryClient = new QueryClient();
    const payload = current.discriminants.eventPayloads.find(({ type }) => type === "session_liveness_changed") as unknown as Extract<NonNullable<EventEnvelope["payload"]>, { type: "session_liveness_changed" }>;
    const unrelatedSessionId = "70000000-0000-0000-0000-000000000099";
    const unrelatedSession = envelope(null);
    const unrelatedRecovery = envelope(null);
    queryClient.setQueryData(sessionQueryKeys.detail(workspaceId, payload.value.session.id), envelope(null));
    queryClient.setQueryData(sessionQueryKeys.recovery(workspaceId, payload.value.session.id), envelope(null));
    queryClient.setQueryData(sessionQueryKeys.detail(workspaceId, unrelatedSessionId), unrelatedSession);
    queryClient.setQueryData(sessionQueryKeys.recovery(workspaceId, unrelatedSessionId), unrelatedRecovery);
    applyWorkspaceEvent(queryClient, { ...event(["session_observability", "recovery_preview", "board"]), kind: "session_liveness_changed", payload, owner: { kind: "session", id: payload.value.session.id } });
    expect(queryClient.getQueryData<ResponseEnvelope>(sessionQueryKeys.detail(workspaceId, payload.value.session.id))?.result).toEqual({ type: "session_observability", value: payload.value.session });
    expect(queryClient.getQueryData<ResponseEnvelope>(sessionQueryKeys.recovery(workspaceId, payload.value.session.id))?.result).toEqual({ type: "recovery_preview", value: payload.value.recovery });
    expect(queryClient.getQueryData(sessionQueryKeys.detail(workspaceId, unrelatedSessionId))).toBe(unrelatedSession);
    expect(queryClient.getQueryData(sessionQueryKeys.recovery(workspaceId, unrelatedSessionId))).toBe(unrelatedRecovery);
  });

  it("patches only the changed proposal and approval queue while invalidating attention", () => {
    const queryClient = new QueryClient();
    const payload = current.discriminants.eventPayloads.find(({ type }) => type === "proposal_changed") as unknown as Extract<NonNullable<EventEnvelope["payload"]>, { type: "proposal_changed" }>;
    const unrelatedFeatureId = "50000000-0000-0000-0000-000000000099";
    const unrelatedProposal = envelope(null);
    const unrelatedBoard = envelope(null);
    queryClient.setQueryData(proposalQueryKeys.detail(workspaceId, payload.value.proposal.feature.id), envelope(null));
    queryClient.setQueryData(proposalQueryKeys.detail(workspaceId, unrelatedFeatureId), unrelatedProposal);
    queryClient.setQueryData(proposalQueryKeys.queue(workspaceId), envelope({ type: "approval_queue", value: { entries: [], revision: 1 } }));
    const attentionKey = boardQueryKeys.attentionList(workspaceId, { limit: 200, repositoryIds: [], reasonCodes: [] });
    const boardKey = boardQueryKeys.board(workspaceId, { limit: 200, query: null, repositoryIds: [], featureIds: [], statuses: [], laneKeys: [], sort: { field: "key", direction: "ascending" } });
    queryClient.setQueryData(attentionKey, { pages: [envelope(null)], pageParams: [null] });
    queryClient.setQueryData(boardKey, unrelatedBoard);
    applyWorkspaceEvent(queryClient, { ...event(["feature_proposal", "approval_queue", "attention"]), kind: "proposal_changed", payload, owner: { kind: "feature", id: payload.value.proposal.feature.id } });
    expect(queryClient.getQueryData<ResponseEnvelope>(proposalQueryKeys.detail(workspaceId, payload.value.proposal.feature.id))?.result).toEqual({ type: "feature_proposal", value: payload.value.proposal });
    expect(queryClient.getQueryData(proposalQueryKeys.detail(workspaceId, unrelatedFeatureId))).toBe(unrelatedProposal);
    const queue = queryClient.getQueryData<ResponseEnvelope>(proposalQueryKeys.queue(workspaceId))?.result;
    expect(queue?.type).toBe("approval_queue");
    if (queue?.type !== "approval_queue") throw new Error("approval queue missing");
    expect(queue.value.entries).toEqual([payload.value.queueItem]);
    expect(queryClient.getQueryState(attentionKey)?.isInvalidated).toBe(true);
    expect(queryClient.getQueryState(boardKey)?.isInvalidated).not.toBe(true);
  });

  it("patches only the affected Work item and its canonical evidence", () => {
    const queryClient = new QueryClient();
    const payload = current.discriminants.eventPayloads.find(({ type }) => type === "work_item_changed") as unknown as Extract<NonNullable<EventEnvelope["payload"]>, { type: "work_item_changed" }>;
    const unrelatedWorkItemId = "60000000-0000-0000-0000-000000000099";
    const unrelatedSessionId = "70000000-0000-0000-0000-000000000099";
    const unrelatedCheckoutId = "b0000000-0000-0000-0000-000000000099";
    const unrelatedDetail = envelope(null);
    const unrelatedSession = envelope(null);
    const unrelatedCheckout = envelope(null);
    queryClient.setQueryData(workItemQueryKeys.detail(workspaceId, payload.value.detail.workItem.id), envelope(null));
    queryClient.setQueryData(workItemQueryKeys.detail(workspaceId, unrelatedWorkItemId), unrelatedDetail);
    queryClient.setQueryData(sessionQueryKeys.detail(workspaceId, unrelatedSessionId), unrelatedSession);
    queryClient.setQueryData(checkoutQueryKeys.detail(workspaceId, unrelatedCheckoutId), unrelatedCheckout);
    queryClient.setQueryData(hierarchyQueryKeys.workspace(workspaceId), envelope(null));
    const attentionKey = boardQueryKeys.attentionList(workspaceId, { limit: 200, repositoryIds: [], reasonCodes: [] });
    queryClient.setQueryData(attentionKey, { pages: [envelope(null)], pageParams: [null] });
    applyWorkspaceEvent(queryClient, { ...event(["work_item_detail", "board", "attention", "workspace_hierarchy", "checkout_observability", "session_observability"]), kind: "work_item_changed", payload, owner: { kind: "work_item", id: payload.value.detail.workItem.id } });
    expect(queryClient.getQueryData<ResponseEnvelope>(workItemQueryKeys.detail(workspaceId, payload.value.detail.workItem.id))?.result).toEqual({ type: "work_item_detail", value: payload.value.detail });
    expect(queryClient.getQueryData(workItemQueryKeys.detail(workspaceId, unrelatedWorkItemId))).toBe(unrelatedDetail);
    expect(queryClient.getQueryData(sessionQueryKeys.detail(workspaceId, unrelatedSessionId))).toBe(unrelatedSession);
    expect(queryClient.getQueryData(checkoutQueryKeys.detail(workspaceId, unrelatedCheckoutId))).toBe(unrelatedCheckout);
    expect(queryClient.getQueryState(attentionKey)?.isInvalidated).toBe(true);
    expect(queryClient.getQueryState(hierarchyQueryKeys.workspace(workspaceId))?.isInvalidated).toBe(true);
  });
});
