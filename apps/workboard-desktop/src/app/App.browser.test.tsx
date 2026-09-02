import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { page, userEvent } from "@vitest/browser/context";
import { expect, it, vi } from "vitest";
import "vitest-browser-react";

import type { DaemonFacade } from "../core/daemon";
import { App } from "./App";
import { BootstrapStatus } from "../features/bootstrap/components/BootstrapScreen";
import { useNavigationStore } from "../features/navigation/store/navigationStore";
import "../styles.css";

const { fakeDaemon, ids } = vi.hoisted(() => {
  const workspaceId = "20000000-0000-0000-0000-000000000001";
  const serviceId = "30000000-0000-0000-0000-000000000001";
  const toolingId = "30000000-0000-0000-0000-000000000002";
  const epicId = "40000000-0000-0000-0000-000000000001";
  const featureId = "50000000-0000-0000-0000-000000000001";
  const workItemId = "60000000-0000-0000-0000-000000000001";
  const requestId = "10000000-0000-0000-0000-000000000001";
  const response = (result: unknown, actions: unknown[] = []) => ({ protocolVersion: 8, requestId, correlationId: requestId, workspaceId, authoritativeRevision: 4, serverTimestamp: "2026-08-30T12:00:00Z", result, error: null, diagnostics: [], availableActions: actions, partialOutcomes: [] });
  const workItem = { id: workItemId, featureId, key: "WI-1", slug: "restore-shell", title: "Restore the shell" };
  const feature = { id: featureId, epicId, slug: "cross-repo", title: "Cross repository feature" };
  const repositories = [{ id: serviceId, workspaceId, slug: "service", title: "Service repository" }];
  const card = {
    workItem,
    feature,
    status: "blocked",
    laneKey: "blocked",
    lanePosition: 1,
    laneCount: 1,
    dependencyReadiness: "ready",
    blockedBy: [{ workItem: { id: "60000000-0000-0000-0000-000000000002", featureId, key: "WI-5", slug: "prerequisite", title: "Prerequisite" }, status: "in_progress" }],
    parallelReadiness: { groupKey: "group", readyCount: 1, waitingCount: 0 },
    repositories,
    sessionSummary: { total: 0, active: 0, idle: 0, unknown: 0, providers: [] },
    checkoutIds: [],
    sessionIds: [],
    attentionReasons: [],
    revision: 4,
    availableActions: [],
  };
  const lanes = ["backlog", "ready", "in_progress", "blocked", "review", "done"].map((key, index) => ({ key, title: key, position: index + 1, totalCount: key === "blocked" ? 1 : 0 }));
  return {
    ids: { workspaceId, serviceId, toolingId, epicId, featureId, workItemId },
    fakeDaemon: {
      handshake: async () => ({ state: "read_only", refusal: null, subscriptions: [{ workspaceId }] }),
      workspaceSummary: async () => response({ type: "workspace_summary", value: { workspace: { id: workspaceId, slug: "workspace", title: "Workspace" }, repositoryCount: 2, epicCount: 1, featureCount: 1, workItemCount: 1, sessionCount: 0 } }) as never,
      hierarchyChildren: async () => response(null) as never,
      workspaceHierarchy: async () => response({ type: "workspace_hierarchy", value: {
        workspace: { id: workspaceId, slug: "workspace", title: "Workspace" },
        repositories: [...repositories, { id: toolingId, workspaceId, slug: "tooling", title: "Tooling repository" }],
        epics: [{ epic: { id: epicId, workspaceId, slug: "delivery", title: "Delivery" }, repositoryIds: [serviceId] }],
        features: [{ feature, repositoryIds: [serviceId] }],
        workItems: [{ workItem, repositoryIds: [serviceId], status: "blocked" }],
        recentEntities: [{ kind: "feature", id: featureId }],
        focusedEntity: { kind: "feature", id: featureId },
      } }) as never,
      boardViews: async () => response({ type: "board_views", value: [] }, [{ code: "save_board_view", available: false, unavailableReason: { code: "read_only", message: "Saving is disabled by the daemon." }, expectedRevision: 4 }]) as never,
      boardView: async () => response(null) as never,
      board: async () => response({ type: "board", value: { lanes, cards: [card], nextCursor: null, totalCount: 1, revision: 4 } }) as never,
      attention: async () => response({ type: "attention", value: { entries: [], nextCursor: null, totalCount: 0, revision: 4 } }) as never,
      approvalQueue: async () => response({ type: "approval_queue", value: { entries: [], revision: 4 } }) as never,
      featureProposal: async () => response(null) as never,
      workItemDetail: async () => response(null) as never,
      repositoryObservability: async () => response(null) as never,
      checkoutObservability: async () => response(null) as never,
      sessionObservability: async () => response(null) as never,
      recoveryPreview: async () => response(null) as never,
      execute: async () => response(null) as never,
      subscribe: async () => ({ cancel: async () => undefined }),
    } as DaemonFacade,
  };
});

vi.mock("../core/daemon", async (importOriginal) => ({ ...(await importOriginal<typeof import("../core/daemon")>()), daemon: fakeDaemon }));

function render(path: string) {
  window.history.replaceState(null, "", path);
  window.dispatchEvent(new PopStateEvent("popstate"));
  useNavigationStore.setState({ filter: "", overrides: {} });
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  page.render(<QueryClientProvider client={queryClient}><App /></QueryClientProvider>);
}

it("navigates repository to Epic to Feature through the persistent sidebar", async () => {
  document.body.style.zoom = "2";
  render("/");
  await expect.element(page.getByRole("heading", { name: "Workspace overview" })).toBeVisible();
  const sidebar = page.getByRole("complementary", { name: "Workspace navigation" });
  await expect.element(sidebar).toBeVisible();

  await page.getByRole("button", { name: "Expand Service repository" }).click();
  await page.getByRole("button", { name: "Expand Delivery" }).click();
  await sidebar.getByRole("link", { name: "Cross repository feature" }).click();
  await expect.element(page.getByRole("heading", { name: "Cross repository feature", level: 1 })).toBeVisible();
  await expect.element(page.getByRole("navigation", { name: "Breadcrumbs" })).toBeVisible();
  await expect.element(page.getByRole("complementary", { name: "Workspace navigation" })).toBeVisible();
  await expect.element(page.getByLabelText("Work item board")).toBeVisible();
  document.body.style.zoom = "";
});

it("keeps Detail and Proposal as tabs on the Feature page and opens a Work item as a full page", async () => {
  render(`/workspaces/${ids.workspaceId}/features/${ids.featureId}`);
  const tabs = page.getByRole("navigation", { name: "Feature views" });
  await expect.element(tabs.getByRole("link", { name: "Detail" })).toBeVisible();
  await tabs.getByRole("link", { name: "Detail" }).click();
  await expect.element(page.getByRole("heading", { name: "Work items", level: 2 })).toBeVisible();
  await tabs.getByRole("link", { name: "Proposal", exact: true }).click();
  await expect.element(page.getByText(/proposal/i).first()).toBeVisible();

  render(`/workspaces/${ids.workspaceId}/work-items/${ids.workItemId}`);
  const breadcrumbs = page.getByRole("navigation", { name: "Breadcrumbs" });
  await expect.element(breadcrumbs).toBeVisible();
  await expect.element(breadcrumbs.getByRole("link", { name: "Service repository" })).toBeVisible();
  await expect.element(breadcrumbs.getByRole("link", { name: "Delivery" })).toBeVisible();
  await expect.element(breadcrumbs.getByText("WI-1 Restore the shell").first()).toBeVisible();
});

it("filters the sidebar tree without flattening it", async () => {
  render("/");
  await expect.element(page.getByRole("complementary", { name: "Workspace navigation" })).toBeVisible();
  await page.getByRole("textbox", { name: "Filter navigation" }).fill("cross repository");
  const sidebar = page.getByRole("complementary", { name: "Workspace navigation" });
  await expect.element(sidebar.getByRole("link", { name: "Cross repository feature" })).toBeVisible();
  await expect.element(sidebar.getByRole("link", { name: "Tooling repository" })).not.toBeInTheDocument();
  await page.getByRole("textbox", { name: "Filter navigation" }).fill("nothing matches");
  await expect.element(page.getByText("No repositories match this filter.")).toBeVisible();
});

it("announces disconnected and incompatible states through a single landmark", async () => {
  page.render(<BootstrapStatus state="disconnected" />);
  await expect.element(page.getByRole("main")).toBeVisible();
  await expect.element(page.getByRole("heading", { name: "Workboard is unavailable" })).toBeVisible();
  page.render(<BootstrapStatus state="incompatible" />);
  await expect.element(page.getByRole("heading", { name: "Desktop and Workboard are incompatible" })).toBeVisible();
  page.render(<BootstrapStatus state="resyncing" />);
  await expect.element(page.getByRole("heading", { name: "Resynchronizing Workboard" })).toBeVisible();
  const css = Array.from(document.styleSheets)
    .flatMap((styleSheet) => Array.from(styleSheet.cssRules))
    .map((rule) => rule.cssText)
    .join("\n");
  expect(css).toContain("(prefers-reduced-motion: reduce)");
  expect(css).toContain("(forced-colors: active)");
});

it("opens the daemon-owned proposal queue by keyboard and restores route focus", async () => {
  render("/");
  const proposals = page.getByRole("link", { name: "Proposals" });
  await expect.element(proposals).toBeVisible();
  (proposals.element() as HTMLElement).focus();
  await userEvent.keyboard("{Enter}");
  await expect.element(page.getByText("No Feature proposals currently require review.")).toBeVisible();
  await vi.waitFor(() => expect(document.activeElement?.id).toBe("main-content"));
});

it("reports a deep link the authoritative hierarchy no longer resolves", async () => {
  render(`/workspaces/${ids.workspaceId}/features/50000000-0000-0000-0000-000000000099`);
  await expect.element(page.getByRole("heading", { name: "Feature not found" })).toBeVisible();
});
