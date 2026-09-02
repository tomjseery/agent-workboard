import { describe, expect, it } from "vitest";

import type { WorkspaceHierarchy } from "../../../core/generated";
import { breadcrumbTrail, type BreadcrumbTarget } from "./breadcrumbTrail";
import { entityPresentations } from "../types/presentation";

const workspaceId = "20000000-0000-0000-0000-000000000001";
const alpha = "30000000-0000-0000-0000-000000000001";
const zulu = "30000000-0000-0000-0000-000000000002";
const epicId = "40000000-0000-0000-0000-000000000001";
const featureId = "50000000-0000-0000-0000-000000000001";
const detachedFeatureId = "50000000-0000-0000-0000-000000000002";
const workItemId = "60000000-0000-0000-0000-000000000001";

const hierarchy: WorkspaceHierarchy = {
  workspace: { id: workspaceId, slug: "workspace", title: "Concertable" },
  repositories: [
    { id: zulu, workspaceId, slug: "zulu", title: "Zulu" },
    { id: alpha, workspaceId, slug: "alpha", title: "Alpha" },
  ],
  epics: [{ epic: { id: epicId, workspaceId, slug: "delivery", title: "Delivery" }, repositoryIds: [zulu, alpha] }],
  features: [
    { feature: { id: featureId, epicId, slug: "board", title: "Board" }, repositoryIds: [zulu, alpha] },
    { feature: { id: detachedFeatureId, epicId, slug: "detached", title: "Detached" }, repositoryIds: [] },
  ],
  workItems: [{ workItem: { id: workItemId, featureId, key: "WI-1", slug: "wi-1", title: "First" }, repositoryIds: [zulu, alpha], status: "ready" }],
  recentEntities: [],
  focusedEntity: null,
};

const titles = (target: BreadcrumbTarget) => breadcrumbTrail(hierarchy, target).map((step) => step.title);

describe("breadcrumb trail", () => {
  it("walks repository, Epic, Feature, and Work item for every entity kind", () => {
    expect(Object.keys(entityPresentations)).toEqual(["repository", "epic", "feature", "work_item"]);
    expect(titles({ kind: "repository", id: alpha })).toEqual(["Concertable", "Alpha"]);
    expect(titles({ kind: "epic", id: epicId })).toEqual(["Concertable", "Alpha", "Delivery"]);
    expect(titles({ kind: "feature", id: featureId })).toEqual(["Concertable", "Alpha", "Delivery", "Board"]);
    expect(titles({ kind: "work_item", id: workItemId })).toEqual(["Concertable", "Alpha", "Delivery", "Board", "WI-1 First"]);
  });

  it("omits an ancestor the authoritative hierarchy does not record", () => {
    expect(titles({ kind: "feature", id: detachedFeatureId })).toEqual(["Concertable", "Delivery", "Detached"]);
    expect(titles({ kind: "work_item", id: "60000000-0000-0000-0000-000000000099" })).toEqual(["Concertable"]);
  });

  it("names each step with its own entity kind", () => {
    expect(breadcrumbTrail(hierarchy, { kind: "work_item", id: workItemId }).map((step) => step.kind)).toEqual(["workspace", "repository", "epic", "feature", "work_item"]);
  });
});
