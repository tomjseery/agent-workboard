import { describe, expect, it } from "vitest";

import { epicViewSchema, featureTabSchema, repositoryViewSchema } from "./search";

describe("route search validation", () => {
  it("defaults every level to its board and refuses unknown views", () => {
    expect(repositoryViewSchema.parse({})).toEqual({ view: "board" });
    expect(epicViewSchema.parse({})).toEqual({ view: "board" });
    expect(featureTabSchema.parse({})).toEqual({ tab: "board" });
    expect(repositoryViewSchema.parse({ view: "shell" })).toEqual({ view: "board" });
    expect(epicViewSchema.parse({ view: "evidence" })).toEqual({ view: "board" });
    expect(featureTabSchema.parse({ tab: 7 })).toEqual({ tab: "board" });
  });

  it("keeps every advertised view reachable by deep link", () => {
    expect(repositoryViewSchema.parse({ view: "features" })).toEqual({ view: "features" });
    expect(repositoryViewSchema.parse({ view: "evidence" })).toEqual({ view: "evidence" });
    expect(epicViewSchema.parse({ view: "features" })).toEqual({ view: "features" });
    expect(featureTabSchema.parse({ tab: "detail" })).toEqual({ tab: "detail" });
    expect(featureTabSchema.parse({ tab: "proposal" })).toEqual({ tab: "proposal" });
  });
});
