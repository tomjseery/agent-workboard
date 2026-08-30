import { describe, expect, it } from "vitest";

import { bootstrapPresentations } from "./BootstrapScreen";

describe("bootstrap presentation", () => {
  it("covers every permitted bootstrap state without workflow controls", () => {
    expect(Object.keys(bootstrapPresentations)).toEqual([
      "connecting",
      "disconnected",
      "incompatible",
      "read_only",
      "resyncing",
      "ready",
    ]);
    expect(JSON.stringify(bootstrapPresentations)).not.toMatch(
      /approve|execute|launch|resume|checkpoint/i,
    );
  });
});
