import { describe, expect, it } from "vitest";

import { createWorkItemStateSchema } from "./workItemStateSchema";

const valid = {
  schemaVersion: 1 as const,
  expectedStateRevision: 2,
  expectedDocumentRevision: 3,
  currentState: "Implementation is complete.",
  nextAction: { kind: "review" as const, description: "Review the candidate." },
  blockers: [],
  decisions: [],
  verification: [],
  review: { status: "ready" as const, evidence: ["candidate abc123"] },
  delivery: { status: "not_started" as const, evidence: [] },
  status: "review" as const,
  terminalIntent: null,
};

describe("workItemStateSchema", () => {
  const schema = createWorkItemStateSchema("in_progress");

  it("accepts a complete revision-checked state", () => {
    expect(schema.parse(valid).currentState).toBe("Implementation is complete.");
  });

  it("requires structured blocker evidence for blocked status", () => {
    const result = schema.safeParse({ ...valid, status: "blocked", nextAction: { kind: "blocked", description: "Wait." } });
    expect(result.success).toBe(false);
  });

  it("keeps terminal intent aligned with status", () => {
    expect(schema.safeParse({ ...valid, status: "cancelled", terminalIntent: "cancel" }).success).toBe(true);
    expect(schema.safeParse({ ...valid, status: "cancelled", terminalIntent: null }).success).toBe(false);
    expect(schema.safeParse({ ...valid, status: "ready", terminalIntent: "complete" }).success).toBe(false);
  });

  it("rejects a transition not accepted from the authoritative status", () => {
    expect(createWorkItemStateSchema("backlog").safeParse(valid).success).toBe(false);
  });
});
