import { describe, expect, it } from "vitest";

import { hierarchySearchSchema } from "./search";

describe("hierarchy route search", () => {
  it("parses valid deep-link search and rejects malformed values to the safe default", () => {
    expect(hierarchySearchSchema.parse({ q: "feature" })).toEqual({ q: "feature" });
    expect(hierarchySearchSchema.parse({ q: 42 })).toEqual({ q: "" });
    expect(hierarchySearchSchema.parse({ q: "x".repeat(201) })).toEqual({ q: "" });
  });
});
