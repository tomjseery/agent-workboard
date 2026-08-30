import { beforeEach, describe, expect, it, vi } from "vitest";

import { boardViewDefinitionSchema } from "../schemas/boardViewDefinitionSchema";
import { useSavedViewDraftStore } from "../store/savedViewDraftStore";
import { buildBoardViewDefinition } from "./useSavedViewEditor";

const workspaceId = "20000000-0000-4000-8000-000000000001";
const repositoryId = "30000000-0000-4000-8000-000000000001";
const viewId = "a0000000-0000-4000-8000-000000000001";

describe("saved-view draft boundary", () => {
  beforeEach(() => {
    vi.stubGlobal("crypto", { randomUUID: () => viewId });
    useSavedViewDraftStore.setState({ draft: undefined });
  });

  it("keeps unsaved transitions private and shapes the parsed daemon request", () => {
    const actions = useSavedViewDraftStore.getState();
    actions.begin(workspaceId);
    actions.setTitle("Service view");
    actions.setQuery("  active  ");
    actions.toggleRepository(repositoryId);
    actions.setGroupingKind("repository");
    actions.setDensity("compact");
    actions.toggleStatus("in_progress");
    actions.setSortField("key");
    actions.setSortDirection("descending");
    const draft = useSavedViewDraftStore.getState().draft;
    expect(draft).toBeDefined();
    const parsed = boardViewDefinitionSchema.parse(buildBoardViewDefinition(draft!));
    expect(parsed).toEqual({ id: viewId, workspaceId, title: "Service view", filters: { query: "active", repositoryIds: [repositoryId], statuses: ["in_progress"] }, grouping: { kind: "repository", lanes: [{ key: "repository", title: "Repository" }] }, sort: { field: "key", direction: "descending" }, density: "compact", revision: 0 });
  });

  it("reports inline title validation without manufacturing a saved result", () => {
    useSavedViewDraftStore.getState().begin(workspaceId);
    const draft = useSavedViewDraftStore.getState().draft!;
    expect(boardViewDefinitionSchema.safeParse(buildBoardViewDefinition(draft)).success).toBe(false);
    expect(useSavedViewDraftStore.getState().draft).toEqual(draft);
  });
});
