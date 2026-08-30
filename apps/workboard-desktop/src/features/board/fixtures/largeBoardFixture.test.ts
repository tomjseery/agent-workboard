import { describe, expect, it } from "vitest";

import { createLargeBoardFixture } from "./largeBoardFixture";

describe("large board fixture", () => {
  it("contains deterministic mixed-repository dependency and attention evidence", () => {
    const fixture = createLargeBoardFixture();
    expect(fixture.repositories).toHaveLength(100);
    expect(fixture.features).toBe(1_000);
    expect(fixture.cards).toHaveLength(10_000);
    expect(new Set(fixture.cards.map((card) => card.workItem.id))).toHaveLength(10_000);
    expect(fixture.cards.every((card) => card.repositories.length === 2)).toBe(true);
    expect(new Set(fixture.cards.flatMap((card) => card.sessionSummary.providers))).toEqual(new Set(["claude", "codex"]));
    expect(new Set(fixture.attentionEntries.flatMap((entry) => entry.reasons.map((reason) => reason.code))).size).toBe(8);
    expect(fixture.cards.some((card) => card.blockedBy.length > 0)).toBe(true);
  });
});
