import { expect, it } from "vitest";

import type { Session } from "../../../core/contracts";
import { isResumable, orderSessions } from "./SessionControls";

function session(
  id: string,
  overrides: Partial<Session> = {},
): Session {
  return {
    id,
    provider: "codex",
    role: "work_item_execution",
    owner: { kind: "work_item", id: "60000000-0000-0000-0000-000000000001", title: "Item" },
    authoritativeProfile: null,
    authoritativeModel: null,
    profileEvidence: { state: "not_loaded", code: "x", message: "x", observedAt: null },
    bindingState: "stopped",
    liveness: { state: "stopped", stale: false, observedAt: null, expiresAt: null, evidence: { state: "not_loaded", code: "x", message: "x", observedAt: null } },
    restoreState: "not_tracked",
    lastActivityAt: null,
    checkoutId: null,
    resumability: "unknown",
    primaryWriter: "not_applicable",
    revision: 1,
    diagnostics: [],
    ...overrides,
  } as Session;
}

it("orders current and live sessions ahead of stopped ones, then by most recent activity", () => {
  const stopped = session("c", { lastActivityAt: "2026-08-30T09:00:00Z" });
  const idle = session("b", {
    liveness: { ...session("b").liveness, state: "idle" },
    lastActivityAt: "2026-08-31T09:00:00Z",
  });
  const current = session("a", { bindingState: "current", lastActivityAt: "2026-08-29T09:00:00Z" });

  expect(orderSessions([stopped, idle, current]).map((entry) => entry.id)).toEqual(["a", "b", "c"]);
});

it("breaks an activity tie deterministically by identity rather than input order", () => {
  const left = session("11111111-0000-0000-0000-000000000001", { lastActivityAt: "2026-08-31T09:00:00Z" });
  const right = session("22222222-0000-0000-0000-000000000002", { lastActivityAt: "2026-08-31T09:00:00Z" });

  expect(orderSessions([right, left]).map((entry) => entry.id)).toEqual(orderSessions([left, right]).map((entry) => entry.id));
});

it("treats only validated or preflight-passed evidence as resumable, and never an already-running session", () => {
  expect(isResumable(session("a", { resumability: "validated" }))).toBe(true);
  expect(isResumable(session("b", { resumability: "preflight_passed" }))).toBe(true);
  expect(isResumable(session("c", { resumability: "unknown" }))).toBe(false);
  expect(isResumable(session("d", { resumability: "missing" }))).toBe(false);
  expect(
    isResumable(
      session("e", {
        resumability: "validated",
        liveness: { ...session("e").liveness, state: "active" },
      }),
    ),
  ).toBe(false);
});
