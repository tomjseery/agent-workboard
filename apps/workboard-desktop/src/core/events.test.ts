import { QueryClient } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";

import type { BoardViewDefinition, EventEnvelope, ResponseEnvelope } from "./generated";
import { applyWorkspaceEvent } from "./events";
import { hierarchyQueryKeys } from "../features/hierarchy/api/hierarchyQueryKeys";
import { savedViewQueryKeys } from "../features/saved-views/api/savedViewQueryKeys";
import { workspaceQueryKeys } from "../features/workspace/api/workspaceQueryKeys";

const workspaceId = "20000000-0000-0000-0000-000000000001";
const viewId = "a0000000-0000-0000-0000-000000000001";
const requestId = "10000000-0000-0000-0000-000000000001";
const view: BoardViewDefinition = { id: viewId, workspaceId, title: "Service", filters: { query: null, repositoryIds: [], statuses: [] }, grouping: { kind: "hierarchy", lanes: [] }, sort: { field: "title", direction: "ascending" }, density: "comfortable", revision: 2 };

function envelope(result: ResponseEnvelope["result"]): ResponseEnvelope {
  return { protocolVersion: 3, requestId, correlationId: requestId, workspaceId, authoritativeRevision: 1, serverTimestamp: "2026-08-30T12:00:00Z", result, error: null, diagnostics: [], availableActions: [], partialOutcomes: [] };
}

function event(queries: EventEnvelope["invalidationScope"] extends infer Scope ? Scope extends { queries: infer Queries } ? Queries : never : never): EventEnvelope {
  return { protocolVersion: 3, eventVersion: 1, workspaceId, sequence: 2, eventId: "90000000-0000-0000-0000-000000000001", occurredAt: "2026-08-30T12:00:01Z", owner: { kind: "workspace", id: workspaceId }, entityRevision: 2, kind: "board_view_saved", payload: { type: "board_view_saved", value: { view } }, invalidationScope: { queries, owners: [{ kind: "workspace", id: workspaceId }] }, operationCorrelationId: requestId, partialOutcomes: [] };
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
});
