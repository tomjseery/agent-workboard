import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { page, userEvent } from "@vitest/browser/context";
import { expect, it, vi } from "vitest";
import "vitest-browser-react";

import { daemon } from "../../../core/daemon";
import type { ResponseEnvelope } from "../../../core/generated";
import current from "../../../core/generated/conformance-current.json";
import { WorkItemDetail } from "./WorkItemDetail";
import "../../../styles.css";

vi.mock("../../../core/daemon", () => ({ daemon: { workItemDetail: vi.fn() } }));

const workspaceId = "20000000-0000-0000-0000-000000000001";
const workItemId = "60000000-0000-0000-0000-000000000001";
const fixture = current.responses.find((candidate) => candidate.result?.type === "work_item_detail") as unknown as ResponseEnvelope;

it("renders hostile durable evidence, blockers, reconciliation, section navigation, and a closed mutation gate", async () => {
  vi.mocked(daemon.workItemDetail).mockResolvedValue({ ...fixture, partialOutcomes: [{ owner: { kind: "work_item", id: workItemId }, code: "checkpoint_partial", succeeded: false, message: "Checkpoint evidence is partial.", reconciliationRequired: true, evidence: [] }] } as never);
  document.body.style.width = "320px";
  document.body.style.zoom = "2";
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  page.render(<QueryClientProvider client={queryClient}><WorkItemDetail workspaceId={workspaceId} workItemId={workItemId} /></QueryClientProvider>);
  await expect.element(page.getByRole("heading", { name: "Fixture Work item", level: 1 })).toBeVisible();
  await expect.element(page.getByText(/<script>alert\('no'\)<\/script>/).first()).toBeVisible();
  await expect.element(page.getByText("Prerequisite is in progress.")).toBeVisible();
  await expect.element(page.getByText("Checkpoint evidence is partial. Reconciliation is required.")).toBeVisible();
  await expect.element(page.getByText("This Work item requires authoritative reconciliation outside Desktop.")).toBeVisible();
  await expect.element(page.getByText("Structured checkpoints unavailable")).toBeVisible();
  await expect.element(page.getByText("41", { exact: true })).toBeVisible();
  await expect.element(page.getByText("3", { exact: true })).toBeVisible();
  const verification = page.getByRole("link", { name: "Verification" });
  (verification.element() as HTMLElement).focus();
  await userEvent.keyboard("{Enter}");
  expect(window.location.hash).toBe("#verification");
  expect(document.querySelector("article script")).toBeNull();
  expect(page.getByRole("textbox").elements()).toHaveLength(0);
  expect(page.getByRole("button", { name: /checkpoint|save|submit|complete/i }).elements()).toHaveLength(0);
  document.body.style.width = "";
  document.body.style.zoom = "";
  window.location.hash = "";
});

it("announces a disconnected detail and independently retries by keyboard", async () => {
  vi.mocked(daemon.workItemDetail).mockReset().mockRejectedValueOnce(new Error("disconnected")).mockResolvedValue(fixture as never);
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  page.render(<QueryClientProvider client={queryClient}><WorkItemDetail workspaceId={workspaceId} workItemId={workItemId} /></QueryClientProvider>);
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
  page.render(<QueryClientProvider client={queryClient}><WorkItemDetail workspaceId={workspaceId} workItemId={workItemId} /></QueryClientProvider>);
  await expect.element(page.getByRole("alert")).toHaveTextContent("No durable state has been reconstructed locally.");
  expect(page.getByRole("textbox").elements()).toHaveLength(0);
});
