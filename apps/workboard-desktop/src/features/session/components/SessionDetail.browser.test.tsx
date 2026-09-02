import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { page, userEvent } from "@vitest/browser/context";
import { expect, it, vi } from "vitest";
import "vitest-browser-react";

import { RouterHarness } from "../../../test/routerHarness";

import { daemon } from "../../../core/daemon";
import { SessionDetail } from "./SessionDetail";
import "../../../styles.css";

vi.mock("../../../core/daemon", () => ({ daemon: { sessionObservability: vi.fn(), recoveryPreview: vi.fn() } }));

const workspaceId = "20000000-0000-0000-0000-000000000001";
const sessionId = "80000000-0000-0000-0000-000000000001";
const requestId = "10000000-0000-0000-0000-000000000001";

function response(result: unknown) {
  return { protocolVersion: 5, requestId, correlationId: requestId, workspaceId, authoritativeRevision: 41, serverTimestamp: "2026-08-31T12:00:00Z", result, error: null, diagnostics: [], availableActions: [], partialOutcomes: [] } as never;
}

it("keeps stale evidence explicit in a narrow dense panel and retries recovery independently by keyboard", async () => {
  vi.mocked(daemon.sessionObservability).mockReset().mockResolvedValue(response({
    type: "session_observability",
    value: {
      id: sessionId,
      provider: "codex",
      role: "work_item_execution",
      owner: { kind: "work_item", id: "60000000-0000-0000-0000-000000000001" },
      authoritativeProfile: null,
      authoritativeModel: null,
      profileEvidence: { state: "not_loaded", code: "profile_evidence_not_loaded", message: "No authoritative profile or model evidence is loaded.", observedAt: null },
      bindingState: "current",
      liveness: { state: "unknown", stale: true, observedAt: "2026-08-31T11:00:00Z", expiresAt: "2026-08-31T11:05:00Z", evidence: { state: "stale", code: "liveness_evidence_stale", message: "The latest evidence has expired.", observedAt: "2026-08-31T11:00:00Z" } },
      restoreState: "tracked",
      lastActivityAt: null,
      checkoutId: null,
      resumability: "missing",
      primaryWriter: "unknown",
      revision: 41,
      diagnostics: [],
    },
  }));
  vi.mocked(daemon.recoveryPreview).mockReset().mockRejectedValueOnce(new Error("disconnected")).mockResolvedValue(response({
    type: "recovery_preview",
    value: {
      sessionId,
      disposition: "conflict",
      conflicts: Array.from({ length: 12 }, (_, index) => ({ code: `conflict-${index}`, severity: "warning", message: `Recovery conflict ${index}`, owner: null })),
      observedAt: "2026-08-31T12:00:00Z",
      stale: true,
      revision: 41,
    },
  }));
  document.body.style.width = "320px";
  document.body.style.zoom = "2";
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  page.render(<RouterHarness><QueryClientProvider client={queryClient}><SessionDetail workspaceId={workspaceId} sessionId={sessionId} /></QueryClientProvider></RouterHarness>);

  await expect.element(page.getByText(/Liveness evidence is stale/)).toBeVisible();
  await expect.element(page.getByText("Unknown", { exact: true }).first()).toBeVisible();
  await expect.element(page.getByText(/Recovery preview is disconnected/)).toBeVisible();
  const retry = page.getByRole("button", { name: "Retry recovery panel" });
  (retry.element() as HTMLElement).focus();
  await userEvent.keyboard("{Enter}");
  await expect.element(page.getByText("Recovery conflict", { exact: true })).toBeVisible();
  await expect.element(page.getByText("Recovery conflict 11")).toBeVisible();
  expect(daemon.sessionObservability).toHaveBeenCalledTimes(1);
  expect(daemon.recoveryPreview).toHaveBeenCalledTimes(2);
  document.body.style.width = "";
  document.body.style.zoom = "";
});
