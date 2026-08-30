import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { page } from "@vitest/browser/context";
import { expect, it, vi } from "vitest";
import "vitest-browser-react";

import type { DaemonFacade } from "../core/daemon";
import { App } from "./App";
import { BootstrapStatus } from "../features/bootstrap/components/BootstrapScreen";
import "../styles.css";

const { fakeDaemon } = vi.hoisted(() => {
  const workspaceId = "20000000-0000-0000-0000-000000000001";
  const repositoryId = "30000000-0000-0000-0000-000000000001";
  const epicId = "40000000-0000-0000-0000-000000000001";
  const featureId = "50000000-0000-0000-0000-000000000001";
  const requestId = "10000000-0000-0000-0000-000000000001";
  const response = (result: unknown, actions: unknown[] = []) => ({ protocolVersion: 4, requestId, correlationId: requestId, workspaceId, authoritativeRevision: 4, serverTimestamp: "2026-08-30T12:00:00Z", result, error: null, diagnostics: [], availableActions: actions, partialOutcomes: [] });
  return { fakeDaemon: {
    handshake: async () => ({ state: "read_only", subscriptions: [{ workspaceId }] }),
    workspaceSummary: async () => response({ type: "workspace_summary", value: { workspace: { id: workspaceId, slug: "workspace", title: "Workspace" }, repositoryCount: 1, epicCount: 1, featureCount: 1, workItemCount: 0, sessionCount: 0 } }) as never,
    hierarchyChildren: async () => response(null) as never,
    workspaceHierarchy: async () => response({ type: "workspace_hierarchy", value: { workspace: { id: workspaceId, slug: "workspace", title: "Workspace" }, repositories: [{ id: repositoryId, workspaceId, slug: "service", title: "Service repository" }], epics: [{ epic: { id: epicId, workspaceId, slug: "delivery", title: "Delivery" }, repositoryIds: [repositoryId] }], features: [{ feature: { id: featureId, epicId, slug: "cross-repo", title: "Cross repository feature" }, repositoryIds: [repositoryId] }], workItems: [], recentEntities: [{ kind: "feature", id: featureId }], focusedEntity: { kind: "feature", id: featureId } } }) as never,
    boardViews: async () => response({ type: "board_views", value: [] }, [{ code: "save_board_view", available: false, unavailableReason: { code: "read_only", message: "Saving is disabled by the daemon." }, expectedRevision: 4 }]) as never,
    boardView: async () => response(null) as never,
    board: async () => response({ type: "board", value: { lanes: [], cards: [], nextCursor: null, totalCount: 0, revision: 4 } }) as never,
    attention: async () => response({ type: "attention", value: { entries: [], nextCursor: null, totalCount: 0, revision: 4 } }) as never,
    execute: async () => response(null) as never,
    subscribe: async () => ({ cancel: async () => undefined }),
  } as DaemonFacade };
});

vi.mock("../core/daemon", async (importOriginal) => ({ ...(await importOriginal<typeof import("../core/daemon")>()), daemon: fakeDaemon }));

it("supports keyboard-first search, routed deep links, focus restoration, and read-only saved-view truth", async () => {
  window.history.replaceState(null, "", "/");
  document.body.style.zoom = "2";
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  page.render(<QueryClientProvider client={queryClient}><App /></QueryClientProvider>);
  await expect.element(page.getByRole("heading", { name: "Workspace overview" })).toBeVisible();
  await expect.element(page.getByText("Saving is disabled by the daemon.")).toBeVisible();
  const search = page.getByRole("textbox", { name: "Search hierarchy" });
  await search.fill("Cross repository");
  await expect.element(page.getByRole("link", { name: "Cross repository feature" }).last()).toBeVisible();
  await page.getByRole("link", { name: "Cross repository feature" }).last().click();
  await expect.element(page.getByRole("heading", { name: "Cross repository feature", level: 1 })).toBeVisible();
  await vi.waitFor(() => expect(document.activeElement?.textContent).toContain("Cross repository feature"));
  await page.getByRole("textbox", { name: "Search hierarchy" }).fill("no matching entity");
  await expect.element(page.getByText("No hierarchy entities match this view.")).toBeVisible();
  window.history.pushState(null, "", "/workspaces/20000000-0000-0000-0000-000000000001/features/50000000-0000-0000-0000-000000000099?q=");
  window.dispatchEvent(new PopStateEvent("popstate"));
  await expect.element(page.getByRole("heading", { name: "Feature not found" })).toBeVisible();
  document.body.style.zoom = "";
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
