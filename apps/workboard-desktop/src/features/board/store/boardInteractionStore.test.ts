import { beforeEach, describe, expect, it } from "vitest";

import { initialBoardFilters, useBoardInteractionStore } from "./boardInteractionStore";

describe("board interaction state", () => {
  beforeEach(() => useBoardInteractionStore.setState({ selectedWorkItemId: undefined, focusedWorkItemId: undefined, filters: initialBoardFilters }));

  it("keeps only selection focus and unsaved controls", () => {
    const store = useBoardInteractionStore.getState();
    store.select("60000000-0000-0000-0000-000000000001");
    store.focus("60000000-0000-0000-0000-000000000002");
    store.toggleLane("cancelled");
    const state = useBoardInteractionStore.getState();
    expect(state.selectedWorkItemId).toContain("0001");
    expect(state.focusedWorkItemId).toContain("0002");
    expect(state.filters.laneKeys).toEqual(["backlog", "ready", "in_progress", "blocked", "review", "done", "cancelled"]);
    expect(state).not.toHaveProperty("cards");
    expect(state).not.toHaveProperty("attention");
  });

  it("starts with Cancelled hidden and keeps lanes in daemon order when toggled", () => {
    expect(useBoardInteractionStore.getState().filters.laneKeys).toEqual(["backlog", "ready", "in_progress", "blocked", "review", "done"]);
    const store = useBoardInteractionStore.getState();
    store.toggleLane("backlog");
    store.toggleLane("cancelled");
    expect(useBoardInteractionStore.getState().filters.laneKeys).toEqual(["ready", "in_progress", "blocked", "review", "done", "cancelled"]);
    store.resetFilters();
    expect(useBoardInteractionStore.getState().filters).toEqual(initialBoardFilters);
  });
});
