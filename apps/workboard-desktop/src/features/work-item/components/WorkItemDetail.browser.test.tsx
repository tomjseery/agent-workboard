import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { page, userEvent } from "@vitest/browser/context";
import { expect, it, vi } from "vitest";
import "vitest-browser-react";

import { RouterHarness } from "../../../test/routerHarness";

import { daemon } from "../../../core/daemon";
import type { DaemonResponse } from "../../../core/contracts";
import current from "../../../core/generated/conformance-current.json";
import { WorkItemDetail } from "./WorkItemDetail";
import "../../../styles.css";

vi.mock("../../../core/daemon", () => ({ daemon: { workItemDetail: vi.fn(), execute: vi.fn() } }));

const workspaceId = "20000000-0000-0000-0000-000000000001";
const workItemId = "60000000-0000-0000-0000-000000000001";
const fixture = current.responses.find((candidate) => candidate.result?.type === "work_item_detail") as unknown as DaemonResponse;

it("renders hostile evidence and saves the complete structured state at narrow high-zoom layout", async () => {
  vi.mocked(daemon.workItemDetail).mockResolvedValue({ ...fixture, partialOutcomes: [{ owner: { kind: "work_item", id: workItemId }, code: "checkpoint_partial", succeeded: false, message: "Checkpoint evidence is partial.", reconciliationRequired: true, evidence: [] }] } as never);
  vi.mocked(daemon.execute).mockResolvedValue(fixture as never);
  document.body.style.width = "320px";
  document.body.style.zoom = "2";
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  page.render(<RouterHarness><QueryClientProvider client={queryClient}><WorkItemDetail workspaceId={workspaceId} workItemId={workItemId} /></QueryClientProvider></RouterHarness>);
  await expect.element(page.getByRole("heading", { name: "Fixture Work item", level: 1 })).toBeVisible();
  await expect.element(page.getByText(/<script>alert\('no'\)<\/script>/).first()).toBeVisible();
  await expect.element(page.getByText("Prerequisite is in progress.")).toBeVisible();
  await expect.element(page.getByText("Checkpoint evidence is partial. Reconciliation is required.")).toBeVisible();
  await expect.element(page.getByText("This Work item requires authoritative reconciliation outside Desktop.")).toBeVisible();
  await expect.element(page.getByRole("heading", { name: "Update durable Work-item state" })).toBeVisible();
  await expect.element(page.getByText("41", { exact: true })).toBeVisible();
  await expect.element(page.getByText("3", { exact: true })).toBeVisible();
  const verification = page.getByRole("link", { name: "Verification" });
  (verification.element() as HTMLElement).focus();
  await userEvent.keyboard("{Enter}");
  expect(window.location.hash).toBe("#verification");
  expect(document.querySelector("article script")).toBeNull();
  await expect.element(page.getByRole("textbox", { name: "Current state" })).toHaveValue("Implementation is ready for review.");
  await userEvent.click(page.getByRole("button", { name: "Save durable state" }));
  await expect.element(page.getByText("Authoritative Work-item state saved.")).toBeVisible();
  expect(daemon.execute).toHaveBeenCalledWith(expect.objectContaining({
    workspaceId,
    expectedRevision: 41,
    command: expect.objectContaining({ type: "checkpoint_work_item" }),
  }));
  document.body.style.width = "";
  document.body.style.zoom = "";
  window.location.hash = "";
});

it("keeps stale state unsaved and offers an authoritative refresh", async () => {
  vi.mocked(daemon.workItemDetail).mockReset().mockResolvedValue(fixture as never);
  vi.mocked(daemon.execute).mockReset().mockResolvedValue({ ...fixture, result: null, error: { code: "work_item_state_revision_stale", message: "The state changed.", severity: "error", retryable: false, validationFields: [], staleRevision: 2, currentRevision: 3, reconciliationOwner: null, correlationId: null, resync: null } } as never);
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  page.render(<RouterHarness><QueryClientProvider client={queryClient}><WorkItemDetail workspaceId={workspaceId} workItemId={workItemId} /></QueryClientProvider></RouterHarness>);
  await userEvent.click(page.getByRole("button", { name: "Save durable state" }));
  await expect.element(page.getByRole("alert").filter({ hasText: "The state changed." })).toBeVisible();
  await expect.element(page.getByRole("button", { name: "Refresh and review changes" })).toBeVisible();
});

it("announces a disconnected detail and independently retries by keyboard", async () => {
  vi.mocked(daemon.workItemDetail).mockReset().mockRejectedValueOnce(new Error("disconnected")).mockResolvedValue(fixture as never);
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  page.render(<RouterHarness><QueryClientProvider client={queryClient}><WorkItemDetail workspaceId={workspaceId} workItemId={workItemId} /></QueryClientProvider></RouterHarness>);
  const retry = page.getByRole("button", { name: "Retry Work-item detail" });
  await expect.element(retry).toBeVisible();
  (retry.element() as HTMLElement).focus();
  await userEvent.keyboard("{Enter}");
  await expect.element(page.getByRole("heading", { name: "Fixture Work item", level: 1 })).toBeVisible();
  expect(daemon.workItemDetail).toHaveBeenCalledTimes(2);
});

it("fails closed for an incompatible detail without reconstructing checkpoint state", async () => {
  vi.mocked(daemon.workItemDetail).mockReset().mockResolvedValue({ ...fixture, result: null, error: { code: "projection_version_unavailable", message: "Unavailable", severity: "error", retryable: false, validationFields: [], staleRevision: null, currentRevision: null, reconciliationOwner: null, correlationId: null, resync: null } } as never);
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  page.render(<RouterHarness><QueryClientProvider client={queryClient}><WorkItemDetail workspaceId={workspaceId} workItemId={workItemId} /></QueryClientProvider></RouterHarness>);
  await expect.element(page.getByRole("alert")).toHaveTextContent("No durable state has been reconstructed locally.");
  expect(page.getByRole("textbox").elements()).toHaveLength(0);
});
