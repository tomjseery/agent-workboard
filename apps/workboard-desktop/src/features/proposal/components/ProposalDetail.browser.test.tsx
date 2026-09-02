import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { page, userEvent } from "@vitest/browser/context";
import { expect, it, vi } from "vitest";
import "vitest-browser-react";

import { RouterHarness } from "../../../test/routerHarness";

import { daemon } from "../../../core/daemon";
import { ApprovalQueue } from "./ApprovalQueue";
import { ProposalDetail } from "./ProposalDetail";
import "../../../styles.css";

vi.mock("../../../core/daemon", () => ({ daemon: { approvalQueue: vi.fn(), featureProposal: vi.fn(), execute: vi.fn() } }));

const workspaceId = "20000000-0000-0000-0000-000000000001";
const featureId = "50000000-0000-0000-0000-000000000001";
const repositoryId = "30000000-0000-0000-0000-000000000001";
const requestId = "10000000-0000-0000-0000-000000000001";

function response(result: unknown, partialOutcomes: unknown[] = []) {
  return { protocolVersion: 6, requestId, correlationId: requestId, workspaceId, authoritativeRevision: 41, serverTimestamp: "2026-08-31T12:00:00Z", result, error: null, diagnostics: [], availableActions: [], partialOutcomes } as never;
}

function proposal(planners = 3) {
  return {
    feature: { id: featureId, epicId: "40000000-0000-0000-0000-000000000001", slug: "hostile", title: "Hostile cross-repository proposal" },
    generation: 2,
    revision: 41,
    proposalHash: "a".repeat(64),
    submittedAt: "2026-08-31T12:00:00Z",
    changedSincePrevious: true,
    featureBody: `# Safe text\n<script>window.evil = true</script>\n[unsafe](javascript:window.evil=true)\n${"Long content ".repeat(800)}`,
    workItems: [{ id: "60000000-0000-0000-0000-000000000001", slug: "delivery", title: "Delivery", body: "<img src=x onerror=window.evil=true>", repositories: [repository("service-a"), repository("service-b", "2")], dependencies: ["foundation"], position: 1 }],
    repositories: [repository("service-a"), repository("service-b", "2")],
    verificationGates: ["Focused checks pass", "Full suite passes"],
    warnings: [{ code: "proposal_changed", severity: "warning", message: "Review the changed proposal." }],
    plannerSessions: Array.from({ length: planners }, (_, index) => ({ id: `70000000-0000-0000-0000-00000000000${index}`, provider: index % 2 === 0 ? "codex" : "claude", role: "feature_planning", bindingState: "current", liveState: index === 0 ? "active" : "idle", lastActivityAt: null })),
    diagnostics: [{ code: "publication_reconciliation_required", severity: "error", message: "Publication requires reconciliation.", owner: { kind: "feature", id: featureId } }],
    workflowState: "reconciliation_required",
    availableActions: ["approve_feature", "request_feature_revision", "reject_feature"].map((code) => ({ code, available: false, unavailableReason: { code: "publication_policy_unavailable", message: "Desktop approval actions are unavailable until policy acceptance." }, expectedRevision: 41 })),
  };
}

function repository(slug: string, suffix = "1") {
  return { id: `${repositoryId.slice(0, -1)}${suffix}`, workspaceId, slug, title: slug };
}

it("renders long hostile changed content as inert text with cross-repository scope and no approval controls", async () => {
  vi.mocked(daemon.featureProposal).mockReset().mockResolvedValue(response({ type: "feature_proposal", value: proposal() }, [{ owner: { kind: "feature", id: featureId }, code: "publication_partial", succeeded: false, message: "Publication is partial.", reconciliationRequired: true, evidence: [] }]));
  document.body.style.width = "320px";
  document.body.style.zoom = "2";
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  page.render(<RouterHarness><QueryClientProvider client={queryClient}><ProposalDetail workspaceId={workspaceId} featureId={featureId} /></QueryClientProvider></RouterHarness>);
  await expect.element(page.getByText(/Proposal changed: review generation 2/)).toBeVisible();
  await expect.element(page.getByText(/<script>window\.evil = true<\/script>/)).toBeVisible();
  await expect.element(page.getByText("Repositories: service-a, service-b")).toBeVisible();
  await expect.element(page.getByText("Publication requires reconciliation.")).toBeVisible();
  await expect.element(page.getByText("Publication is partial. Reconciliation is required.")).toBeVisible();
  await expect.element(page.getByText("Desktop approval actions are unavailable until policy acceptance.").first()).toBeVisible();
  await expect.element(page.getByText("codex", { exact: true }).first()).toBeVisible();
  expect(document.querySelector('[aria-labelledby="proposal-title"] script')).toBeNull();
  expect(document.querySelector('[aria-labelledby="proposal-title"] a[href^="javascript:"]')).toBeNull();
  expect((page.getByRole("button", { name: "Approve and publish" }).element() as HTMLButtonElement).disabled).toBe(true);
  expect((page.getByRole("button", { name: "Request revision" }).element() as HTMLButtonElement).disabled).toBe(true);
  expect(daemon.execute).not.toHaveBeenCalled();
  expect((window as unknown as { evil?: boolean }).evil).not.toBe(true);
  document.body.style.width = "";
  document.body.style.zoom = "";
});

it("supports empty queues and independently retries a disconnected proposal by keyboard", async () => {
  vi.mocked(daemon.approvalQueue).mockReset().mockResolvedValue(response({ type: "approval_queue", value: { entries: [], revision: 41 } }));
  vi.mocked(daemon.featureProposal).mockReset().mockRejectedValueOnce(new Error("disconnected")).mockResolvedValue(response({ type: "feature_proposal", value: proposal(0) }));
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  page.render(<RouterHarness><QueryClientProvider client={queryClient}><ApprovalQueue workspaceId={workspaceId} /><ProposalDetail workspaceId={workspaceId} featureId={featureId} /></QueryClientProvider></RouterHarness>);
  await expect.element(page.getByText("No Feature proposals currently require review.")).toBeVisible();
  const retry = page.getByRole("button", { name: "Retry proposal detail" });
  (retry.element() as HTMLElement).focus();
  await userEvent.keyboard("{Enter}");
  await expect.element(page.getByText("No planner sessions are recorded.")).toBeVisible();
  expect(daemon.approvalQueue).toHaveBeenCalledTimes(1);
  expect(daemon.featureProposal).toHaveBeenCalledTimes(2);
});

function actionable() {
  return {
    ...proposal(1),
    workflowState: "awaiting_approval",
    availableActions: [
      { code: "approve_feature", available: true, unavailableReason: null, expectedRevision: 41 },
      { code: "request_feature_revision", available: true, unavailableReason: null, expectedRevision: 41 },
      { code: "reject_feature", available: false, unavailableReason: { code: "terminal_rejection_unavailable", message: "Rejecting a proposal outright is unavailable; request a revision instead." }, expectedRevision: 41 },
    ],
  };
}

it("approves, validates revision feedback before sending, and reports a stale revision without claiming success", async () => {
  vi.mocked(daemon.featureProposal).mockReset().mockResolvedValue(response({ type: "feature_proposal", value: actionable() }));
  vi.mocked(daemon.execute).mockReset().mockResolvedValue(response({ type: "feature_proposal", value: actionable() }));
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  page.render(<RouterHarness><QueryClientProvider client={queryClient}><ProposalDetail workspaceId={workspaceId} featureId={featureId} /></QueryClientProvider></RouterHarness>);

  await expect.element(page.getByText("Rejecting a proposal outright is unavailable; request a revision instead.")).toBeVisible();

  const requestRevision = page.getByRole("button", { name: "Request revision" });
  (requestRevision.element() as HTMLElement).focus();
  await userEvent.keyboard("{Enter}");
  await expect.element(page.getByText("Describe what the planner must change before requesting a revision.")).toBeVisible();
  expect(daemon.execute).not.toHaveBeenCalled();

  await userEvent.fill(page.getByLabelText("Revision feedback"), "  Split the migration item.  ");
  await userEvent.click(requestRevision);
  await vi.waitFor(() => expect(daemon.execute).toHaveBeenCalledTimes(1));
  expect(vi.mocked(daemon.execute).mock.calls[0][0]).toMatchObject({
    workspaceId,
    expectedRevision: 41,
    command: { type: "request_feature_revision", value: { featureId, feedback: "Split the migration item." } },
  });

  await userEvent.click(page.getByRole("button", { name: "Approve and publish" }));
  await vi.waitFor(() => expect(daemon.execute).toHaveBeenCalledTimes(2));
  expect(vi.mocked(daemon.execute).mock.calls[1][0]).toMatchObject({
    workspaceId,
    expectedRevision: 41,
    command: { type: "approve_feature", value: { featureId } },
  });
});

it("surfaces a stale-revision refusal and a partial publication outcome instead of reporting success", async () => {
  vi.mocked(daemon.featureProposal).mockReset().mockResolvedValue(response({ type: "feature_proposal", value: actionable() }));
  vi.mocked(daemon.execute).mockReset().mockResolvedValue({
    ...(response(null) as unknown as Record<string, unknown>),
    error: { code: "stale_revision", message: "Workspace revision 41 is stale; current revision is 42.", severity: "error", retryable: false, validationFields: [], staleRevision: 41, currentRevision: 42, reconciliationOwner: null, correlationId: requestId, resync: null },
    partialOutcomes: [{ owner: { kind: "feature", id: featureId }, code: "workflow_document_changed", succeeded: false, message: "Publication did not complete.", reconciliationRequired: true, evidence: [] }],
  } as never);
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  page.render(<RouterHarness><QueryClientProvider client={queryClient}><ProposalDetail workspaceId={workspaceId} featureId={featureId} /></QueryClientProvider></RouterHarness>);
  await userEvent.click(page.getByRole("button", { name: "Approve and publish" }));
  await expect.element(page.getByText(/Workspace revision 41 is stale/)).toBeVisible();
  await expect.element(page.getByText("Publication did not complete. Reconciliation is required.")).toBeVisible();
});
