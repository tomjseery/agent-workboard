import { describe, expect, it } from "vitest";

import { workItemQueryKeys } from "./workItemQueryKeys";

describe("Work-item query keys", () => {
  it("scopes canonical details below one Workspace without broadening unrelated items", () => {
    const workspaceId = "20000000-0000-0000-0000-000000000001";
    const workItemId = "60000000-0000-0000-0000-000000000001";
    expect(workItemQueryKeys.detail(workspaceId, workItemId)).toEqual(["work-items", workspaceId, workItemId]);
    expect(workItemQueryKeys.workspace(workspaceId)).toEqual(["work-items", workspaceId]);
  });
});
