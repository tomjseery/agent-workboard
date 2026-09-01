import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { page, userEvent } from "@vitest/browser/context";
import { expect, it, vi } from "vitest";
import "vitest-browser-react";

import { daemon } from "../../../core/daemon";
import type {
  AvailableAction,
  RepositoryReference,
  SessionObservabilityProjection,
} from "../../../core/generated";
import { SessionControls } from "./SessionControls";
import "../../../styles.css";

vi.mock("../../../core/daemon", () => ({ daemon: { execute: vi.fn() } }));

const workspaceId = "20000000-0000-0000-0000-000000000001";
const workItemId = "60000000-0000-0000-0000-000000000001";
const requestId = "10000000-0000-0000-0000-000000000001";

function repository(suffix: string, title: string): RepositoryReference {
  return { id: `3000000${suffix}-0000-0000-0000-000000000001`, workspaceId, slug: title.toLowerCase(), title } as RepositoryReference;
}

function session(id: string, overrides: Partial<SessionObservabilityProjection> = {}): SessionObservabilityProjection {
  const evidence = { state: "not_loaded", code: "x", message: "x", observedAt: null };
  return {
    id,
    provider: "codex",
    role: "work_item_execution",
    owner: { kind: "work_item", id: workItemId, title: "Item" },
    authoritativeProfile: null,
    authoritativeModel: null,
    profileEvidence: evidence,
    bindingState: "current",
    liveness: { state: "stopped", stale: false, observedAt: null, expiresAt: null, evidence },
    restoreState: "tracked",
    lastActivityAt: "2026-08-31T09:00:00Z",
    checkoutId: null,
    resumability: "validated",
    primaryWriter: "confirmed_primary",
    revision: 41,
    diagnostics: [],
    ...overrides,
  } as SessionObservabilityProjection;
}

function actions(overrides: Partial<Record<string, Partial<AvailableAction>>> = {}): AvailableAction[] {
  const base: Record<string, AvailableAction> = {
    start_session: { code: "start_session", available: true, unavailableReason: null, expectedRevision: 41 },
    resume_session: { code: "resume_session", available: true, unavailableReason: null, expectedRevision: 41 },
    focus_session: { code: "focus_session", available: false, unavailableReason: { code: "session_focus_unavailable", message: "Focusing a running session is unavailable." }, expectedRevision: 41 },
    follow_up_session: { code: "follow_up_session", available: false, unavailableReason: { code: "session_follow_up_unavailable", message: "Sending a follow-up is unavailable." }, expectedRevision: 41 },
    recover_session: { code: "recover_session", available: false, unavailableReason: { code: "session_recovery_unavailable", message: "Recovery is unavailable from Desktop." }, expectedRevision: 41 },
  };
  for (const [code, patch] of Object.entries(overrides)) base[code] = { ...base[code], ...patch } as AvailableAction;
  return Object.values(base);
}

function render(node: React.ReactNode) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  page.render(<QueryClientProvider client={queryClient}>{node}</QueryClientProvider>);
}

function accepted() {
  return { protocolVersion: 7, requestId, correlationId: requestId, workspaceId, authoritativeRevision: 42, serverTimestamp: "2026-08-31T12:00:00Z", result: null, error: null, diagnostics: [], availableActions: [], partialOutcomes: [] } as never;
}

it("offers Start and nothing to resume when no session is bound", async () => {
  vi.mocked(daemon.execute).mockReset().mockResolvedValue(accepted());
  render(<SessionControls workspaceId={workspaceId} workItemId={workItemId} sessions={[]} repositories={[repository("1", "Service A")]} actions={actions({ resume_session: { available: false, unavailableReason: { code: "no_resumable_session", message: "This Work item has no session with validated resume evidence." } } })} revision={41} />);

  await expect.element(page.getByText("No session is bound to this Work item.")).toBeVisible();
  expect(page.getByRole("button", { name: "Resume" }).elements()).toHaveLength(0);

  await userEvent.click(page.getByRole("button", { name: "Start session" }));
  await vi.waitFor(() => expect(daemon.execute).toHaveBeenCalledTimes(1));
  expect(vi.mocked(daemon.execute).mock.calls[0][0]).toMatchObject({
    workspaceId,
    expectedRevision: 41,
    command: { type: "start_session", value: { workItemId, repositoryId: "30000001-0000-0000-0000-000000000001", provider: "codex" } },
  });
});

it("resumes the single bound session and also offers Start another", async () => {
  vi.mocked(daemon.execute).mockReset().mockResolvedValue(accepted());
  const only = session("70000000-0000-0000-0000-000000000001");
  render(<SessionControls workspaceId={workspaceId} workItemId={workItemId} sessions={[only]} repositories={[repository("1", "Service A")]} actions={actions()} revision={41} />);

  await expect.element(page.getByText("1 bound session.")).toBeVisible();
  await expect.element(page.getByRole("button", { name: "Start another" })).toBeVisible();

  await userEvent.click(page.getByRole("button", { name: "Resume" }));
  await vi.waitFor(() => expect(daemon.execute).toHaveBeenCalledTimes(1));
  expect(vi.mocked(daemon.execute).mock.calls[0][0]).toMatchObject({
    command: { type: "resume_session", value: { sessionId: only.id } },
  });
});

it("orders many sessions, resumes the exact one selected by keyboard, and refuses to duplicate a running session", async () => {
  vi.mocked(daemon.execute).mockReset().mockResolvedValue(accepted());
  const live = session("70000000-0000-0000-0000-00000000000a", {
    bindingState: "pending",
    liveness: { state: "active", stale: false, observedAt: null, expiresAt: null, evidence: { state: "not_loaded", code: "x", message: "x", observedAt: null } },
    lastActivityAt: "2026-08-31T10:00:00Z",
  });
  const resumable = session("70000000-0000-0000-0000-00000000000b", { bindingState: "stopped", lastActivityAt: "2026-08-31T08:00:00Z" });
  const unresumable = session("70000000-0000-0000-0000-00000000000c", { bindingState: "stopped", resumability: "missing", lastActivityAt: "2026-08-31T07:00:00Z" });

  render(<SessionControls workspaceId={workspaceId} workItemId={workItemId} sessions={[unresumable, resumable, live]} repositories={[repository("1", "Service A")]} actions={actions()} revision={41} />);

  await expect.element(page.getByText("3 bound sessions.")).toBeVisible();
  await expect.element(page.getByText("Already running. Workboard will not launch a duplicate.")).toBeVisible();
  await expect.element(page.getByText("No validated resume evidence for this session.")).toBeVisible();

  const radios = page.getByRole("radio").elements() as HTMLInputElement[];
  expect(radios.map((radio) => radio.value)).toEqual([live.id, resumable.id, unresumable.id]);
  expect(radios[0].disabled).toBe(true);
  expect(radios[2].disabled).toBe(true);

  radios[1].focus();
  await userEvent.keyboard(" ");
  await userEvent.click(page.getByRole("button", { name: "Resume" }));
  await vi.waitFor(() => expect(daemon.execute).toHaveBeenCalledTimes(1));
  expect(vi.mocked(daemon.execute).mock.calls[0][0]).toMatchObject({
    command: { type: "resume_session", value: { sessionId: resumable.id } },
  });
});

it("requires a repository choice across many repositories and shows every blocked action with its reason", async () => {
  vi.mocked(daemon.execute).mockReset().mockResolvedValue(accepted());
  render(<SessionControls workspaceId={workspaceId} workItemId={workItemId} sessions={[]} repositories={[repository("1", "Service A"), repository("2", "Service B")]} actions={actions({ start_session: { available: false, unavailableReason: { code: "writer_session_active", message: "A session is already writing in this Work item's checkout." } } })} revision={41} />);

  await expect.element(page.getByText("A session is already writing in this Work item's checkout.")).toBeVisible();
  expect((page.getByRole("button", { name: "Start session" }).element() as HTMLButtonElement).disabled).toBe(true);

  await expect.element(page.getByText("Focusing a running session is unavailable.")).toBeVisible();
  await expect.element(page.getByText("Sending a follow-up is unavailable.")).toBeVisible();
  await expect.element(page.getByText("Recovery is unavailable from Desktop.")).toBeVisible();
  expect(daemon.execute).not.toHaveBeenCalled();
});

it("reports a refused launch without claiming the session started", async () => {
  vi.mocked(daemon.execute).mockReset().mockResolvedValue({
    ...(accepted() as unknown as Record<string, unknown>),
    error: { code: "checkout_writer_active", message: "The Work-item checkout already has a current writer.", severity: "error", retryable: false, validationFields: [], staleRevision: null, currentRevision: 42, reconciliationOwner: null, correlationId: requestId, resync: null },
  } as never);
  render(<SessionControls workspaceId={workspaceId} workItemId={workItemId} sessions={[]} repositories={[repository("1", "Service A")]} actions={actions()} revision={41} />);

  await userEvent.click(page.getByRole("button", { name: "Start session" }));
  await expect.element(page.getByText("The Work-item checkout already has a current writer.")).toBeVisible();
  await expect.element(page.getByText("checkout_writer_active")).toBeVisible();
});
