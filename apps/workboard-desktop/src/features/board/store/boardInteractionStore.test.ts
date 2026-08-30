import { beforeEach, describe, expect, it } from "vitest";

import { useBoardInteractionStore } from "./boardInteractionStore";

describe("board interaction state", () => {
  beforeEach(() => useBoardInteractionStore.setState({ selectedWorkItemId: undefined, focusedWorkItemId: undefined, filters: { query: "", repositoryIds: [], statuses: [], laneKeys: [], sort: { field: "key", direction: "ascending" } } }));

  it("keeps only selection focus and unsaved controls", () => {
    const store = useBoardInteractionStore.getState();
    store.select("60000000-0000-0000-0000-000000000001");
    store.focus("60000000-0000-0000-0000-000000000002");
    store.toggleStatus("blocked");
    store.setLaneKeys(["ready", "blocked"]);
    const state = useBoardInteractionStore.getState();
    expect(state.selectedWorkItemId).toContain("0001");
    expect(state.focusedWorkItemId).toContain("0002");
    expect(state.filters.statuses).toEqual(["blocked"]);
    expect(state.filters.laneKeys).toEqual(["ready", "blocked"]);
    expect(state).not.toHaveProperty("cards");
    expect(state).not.toHaveProperty("attention");
  });
});
