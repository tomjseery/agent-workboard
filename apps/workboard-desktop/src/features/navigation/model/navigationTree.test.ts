import { describe, expect, it } from "vitest";

import type { WorkspaceHierarchy } from "../../../core/generated";
import { buildNavigationTree, navigationPath, unassignedRepositoryTitle } from "./navigationTree";

const workspaceId = "20000000-0000-0000-0000-000000000001";
const service = "30000000-0000-0000-0000-000000000001";
const tooling = "30000000-0000-0000-0000-000000000002";
const delivery = "40000000-0000-0000-0000-000000000001";
const platform = "40000000-0000-0000-0000-000000000002";
const orphanEpic = "40000000-0000-0000-0000-000000000003";
const board = "50000000-0000-0000-0000-000000000001";
const shell = "50000000-0000-0000-0000-000000000002";
const unplanned = "50000000-0000-0000-0000-000000000003";
const workItem = "60000000-0000-0000-0000-000000000001";

const hierarchy: WorkspaceHierarchy = {
  workspace: { id: workspaceId, slug: "workspace", title: "Concertable" },
  repositories: [
    { id: tooling, workspaceId, slug: "tooling", title: "Tooling" },
    { id: service, workspaceId, slug: "service", title: "Service" },
  ],
  epics: [
    { epic: { id: platform, workspaceId, slug: "platform", title: "Platform" }, repositoryIds: [tooling] },
    { epic: { id: delivery, workspaceId, slug: "delivery", title: "Delivery" }, repositoryIds: [service] },
    { epic: { id: orphanEpic, workspaceId, slug: "unstarted", title: "Unstarted" }, repositoryIds: [] },
  ],
  features: [
    { feature: { id: board, epicId: delivery, slug: "board", title: "Board" }, repositoryIds: [service] },
    { feature: { id: shell, epicId: platform, slug: "shell", title: "Shell" }, repositoryIds: [tooling] },
    { feature: { id: unplanned, epicId: delivery, slug: "unplanned", title: "Unplanned" }, repositoryIds: [] },
  ],
  workItems: [
    { workItem: { id: workItem, featureId: board, key: "WI-1", slug: "wi-1", title: "First" }, repositoryIds: [service], status: "in_progress" },
  ],
  recentEntities: [],
  focusedEntity: null,
};

describe("navigation tree", () => {
  it("nests repositories, Epics, and Features in a stable order", () => {
    const tree = buildNavigationTree(hierarchy);
    expect(tree.repositories.map((repository) => repository.title)).toEqual(["Service", "Tooling", unassignedRepositoryTitle]);
    const [serviceNode, toolingNode] = tree.repositories;
    expect(serviceNode?.epics.map((epic) => epic.title)).toEqual(["Delivery"]);
    expect(serviceNode?.epics[0]?.features.map((feature) => feature.title)).toEqual(["Board"]);
    expect(serviceNode?.epics[0]?.features[0]?.workItemCount).toBe(1);
    expect(toolingNode?.epics[0]?.features.map((feature) => feature.title)).toEqual(["Shell"]);
  });

  it("keeps Features and Epics without repository participation reachable", () => {
    const unassigned = buildNavigationTree(hierarchy).repositories.at(-1);
    expect(unassigned?.id).toBeNull();
    expect(unassigned?.epics.map((epic) => epic.title)).toEqual(["Delivery", "Unstarted"]);
    expect(unassigned?.epics[0]?.features.map((feature) => feature.title)).toEqual(["Unplanned"]);
    expect(unassigned?.epics[1]?.features).toEqual([]);
  });

  it("narrows the tree by filter while keeping the path to a match", () => {
    const tree = buildNavigationTree(hierarchy, "shell");
    expect(tree.repositories.map((repository) => repository.title)).toEqual(["Tooling"]);
    expect(tree.repositories[0]?.epics[0]?.features.map((feature) => feature.title)).toEqual(["Shell"]);
    expect(tree.featureCount).toBe(1);
    expect(buildNavigationTree(hierarchy, "service").repositories[0]?.epics[0]?.features.map((feature) => feature.title)).toEqual(["Board"]);
    expect(buildNavigationTree(hierarchy, "nothing matches").repositories).toEqual([]);
  });

  it("resolves the ancestors a route should expand", () => {
    expect(navigationPath(hierarchy, { workItemId: workItem })).toEqual({ repositoryIds: [service], epicId: delivery, featureId: board });
    expect(navigationPath(hierarchy, { featureId: shell })).toEqual({ repositoryIds: [tooling], epicId: platform, featureId: shell });
    expect(navigationPath(hierarchy, { epicId: platform })).toEqual({ repositoryIds: [tooling], epicId: platform, featureId: undefined });
    expect(navigationPath(hierarchy, { repositoryId: service })).toEqual({ repositoryIds: [service], epicId: undefined, featureId: undefined });
    expect(navigationPath(hierarchy, {})).toEqual({ repositoryIds: [], epicId: undefined, featureId: undefined });
  });
});
