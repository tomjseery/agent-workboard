import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { page, userEvent } from "@vitest/browser/context";
import { expect, it, vi } from "vitest";
import "vitest-browser-react";

import { RouterHarness } from "../../../test/routerHarness";

import { daemon } from "../../../core/daemon";
import { createLargeBoardFixture } from "../fixtures/largeBoardFixture";
import { initialBoardFilters, useBoardInteractionStore } from "../store/boardInteractionStore";
import { BoardView } from "./BoardView";
import "../../../styles.css";

vi.mock("../../../core/daemon", () => ({ daemon: { board: vi.fn(), attention: vi.fn() } }));

function boardResponse(fixture: ReturnType<typeof createLargeBoardFixture>, overrides: Partial<Awaited<ReturnType<typeof daemon.board>>> = {}): Awaited<ReturnType<typeof daemon.board>> {
  return { protocolVersion: 4, requestId: "10000000-0000-0000-0000-000000000001", correlationId: "10000000-0000-0000-0000-000000000001", workspaceId: fixture.workspaceId, authoritativeRevision: 1, serverTimestamp: "2026-08-30T12:00:00Z", result: { type: "board", value: { lanes: fixture.lanes, cards: [], nextCursor: null, totalCount: 0, revision: 1 } }, error: null, diagnostics: [], availableActions: [], partialOutcomes: [], ...overrides };
}

it("bounds mounted cards and supports the complete non-drag keyboard path at 200% zoom", async () => {
  const fixture = createLargeBoardFixture();
  vi.mocked(daemon.board).mockReset().mockResolvedValue(boardResponse(fixture, { result: { type: "board", value: { lanes: fixture.lanes, cards: fixture.cards.slice(0, 200), nextCursor: "board:1:200", totalCount: fixture.cards.length, revision: 1 } } }));
  document.body.style.zoom = "2";
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const onOpen = vi.fn();
  page.render(<RouterHarness><QueryClientProvider client={queryClient}><BoardView workspaceId={fixture.workspaceId} onOpenWorkItem={onOpen} /></QueryClientProvider></RouterHarness>);
  await expect.element(page.getByLabelText("Work item board")).toBeVisible();
  await vi.waitFor(() => expect(document.querySelectorAll("[data-board-card]").length).toBeGreaterThan(0));
  expect(document.querySelectorAll("[data-board-card]").length).toBeLessThanOrEqual(200);
  const first = page.getByRole("button", { name: /F0000\/WI0:/ });
  await first.click();
  expect(onOpen).toHaveBeenCalledOnce();
  (first.element() as HTMLElement).focus();
  await userEvent.keyboard(" ");
  await expect.element(page.getByText(/Selected Work item 60000000/)).toBeInTheDocument();
  await userEvent.keyboard("{ArrowDown}");
  await vi.waitFor(() => expect(document.activeElement?.getAttribute("aria-label")).toContain("F0000/WI7"));
  await userEvent.keyboard("{ArrowRight}");
  await vi.waitFor(() => expect(document.activeElement?.getAttribute("aria-label")).toContain("F0000/WI8"));
  await userEvent.keyboard("{End}");
  await vi.waitFor(() => expect(document.activeElement?.getAttribute("aria-label")).toContain("Position 29 of 1429"));
  await userEvent.keyboard("{Enter}");
  expect(onOpen).toHaveBeenCalledTimes(2);
  document.body.style.zoom = "";
});

it("renders loading, empty, partial, incompatible, and transport-error states without local inference", async () => {
  const fixture = createLargeBoardFixture();
  const render = () => page.render(<RouterHarness><QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><BoardView workspaceId={fixture.workspaceId} onOpenWorkItem={() => undefined} /></QueryClientProvider></RouterHarness>);

  vi.mocked(daemon.board).mockReset().mockImplementation(() => new Promise(() => undefined));
  render();
  await expect.element(page.getByText("Loading authoritative board…")).toBeVisible();

  vi.mocked(daemon.board).mockReset().mockResolvedValue(boardResponse(fixture));
  render();
  await expect.element(page.getByText("No Work items match this board view.")).toBeVisible();

  vi.mocked(daemon.board).mockReset().mockResolvedValue(boardResponse(fixture, { partialOutcomes: [{ owner: null, code: "partial", succeeded: false, message: "Partial", reconciliationRequired: false, evidence: [] }] }));
  render();
  await expect.element(page.getByText(/Some authoritative board evidence is partial/)).toBeVisible();

  vi.mocked(daemon.board).mockReset().mockResolvedValue(boardResponse(fixture, { result: null, error: { code: "projection_version_unavailable", message: "Unavailable", severity: "error", retryable: false, validationFields: [], staleRevision: null, currentRevision: null, reconciliationOwner: null, correlationId: null, resync: null } }));
  render();
  await expect.element(page.getByText(/does not provide a compatible board projection/)).toBeVisible();

  vi.mocked(daemon.board).mockReset().mockRejectedValue(new Error("disconnected"));
  render();
  await expect.element(page.getByText(/could not be reached/)).toBeVisible();
});

it("hides Cancelled behind a lane filter and shows dependency readiness as a card badge", async () => {
  const fixture = createLargeBoardFixture();
  useBoardInteractionStore.setState({ filters: initialBoardFilters });
  vi.mocked(daemon.board).mockReset().mockResolvedValue(boardResponse(fixture, { result: { type: "board", value: { lanes: fixture.lanes, cards: fixture.cards.slice(0, 40), nextCursor: null, totalCount: 40, revision: 1 } } }));
  page.render(<RouterHarness><QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><BoardView workspaceId={fixture.workspaceId} onOpenWorkItem={() => undefined} /></QueryClientProvider></RouterHarness>);

  await expect.element(page.getByLabelText("Work item board")).toBeVisible();
  expect(vi.mocked(daemon.board).mock.calls[0]?.[1].laneKeys).toEqual(["backlog", "ready", "in_progress", "blocked", "review", "done"]);
  const cancelled = page.getByRole("checkbox", { name: "Cancelled" });
  await expect.element(cancelled).not.toBeChecked();
  await expect.element(page.getByText("Dependencies ready").first()).toBeVisible();
  await expect.element(page.getByText(/blocked by F0000\/WI0/).first()).toBeVisible();

  await cancelled.click();
  await vi.waitFor(() => expect(vi.mocked(daemon.board).mock.calls.at(-1)?.[1].laneKeys).toContain("cancelled"));
  useBoardInteractionStore.setState({ filters: initialBoardFilters });
});
