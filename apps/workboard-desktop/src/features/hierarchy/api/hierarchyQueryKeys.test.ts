import { describe, expect, it } from "vitest";

import { hierarchyQueryKeys } from "./hierarchyQueryKeys";
import { workspaceQueryKeys } from "../../workspace/api/workspaceQueryKeys";
import { entityPresentations, hierarchyNodeLabels, ownerLabels } from "../model/presentation";

const workspaceId = "20000000-0000-0000-0000-000000000001";

describe("hierarchy query ownership", () => {
  it("keeps canonical keys scoped by stable Workspace ID", () => {
    expect(hierarchyQueryKeys.workspace(workspaceId)).toEqual(["hierarchy", workspaceId]);
    expect(workspaceQueryKeys.detail(workspaceId)).toEqual(["workspaces", workspaceId]);
    expect(JSON.stringify(hierarchyQueryKeys)).not.toMatch(/[A-Z]:|\\|\//);
  });

  it("covers every published hierarchy and owner discriminant", () => {
    expect(Object.keys(hierarchyNodeLabels)).toEqual(["repository", "epic", "feature", "work_item"]);
    expect(Object.keys(ownerLabels)).toEqual(["workspace", "epic", "feature", "work_item"]);
    expect(Object.keys(entityPresentations)).toEqual(["repository", "epic", "feature", "work_item"]);
  });
});
