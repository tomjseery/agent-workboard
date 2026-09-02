import { describe, expect, it } from "vitest";

import { badgeVariants } from "../../../components/ui/badge";
import current from "../../../core/generated/conformance-current.json";
import { defaultLaneKeys, dependencyReadinessPresentations, laneOrder, lanePresentations } from "./presentation";

describe("board presentation tables", () => {
  it("covers every published Work-item status in daemon lane order", () => {
    expect(laneOrder).toEqual(current.discriminants.workItemStatuses);
    expect(Object.keys(lanePresentations)).toEqual(current.discriminants.workItemStatuses);
  });

  it("hides only Cancelled until a filter asks for it", () => {
    expect(defaultLaneKeys).toEqual(["backlog", "ready", "in_progress", "blocked", "review", "done"]);
    expect(laneOrder.filter((status) => lanePresentations[status].hiddenByDefault)).toEqual(["cancelled"]);
  });

  it("covers every published dependency readiness as a badge rather than a lane", () => {
    expect(Object.keys(dependencyReadinessPresentations)).toEqual(current.discriminants.dependencyReadiness);
    expect(laneOrder).not.toContain("waiting");
    for (const badge of Object.values(dependencyReadinessPresentations)) {
      expect(badgeVariants({ tone: badge.tone })).not.toEqual(badgeVariants({ tone: "neutral" }));
    }
  });
});
