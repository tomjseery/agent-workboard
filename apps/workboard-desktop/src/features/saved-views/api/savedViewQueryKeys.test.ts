import { describe, expect, it } from "vitest";

import { savedViewQueryKeys } from "./savedViewQueryKeys";

const workspaceId = "20000000-0000-0000-0000-000000000001";
const viewId = "a0000000-0000-0000-0000-000000000001";

describe("saved-view query keys", () => {
  it("orders list and detail keys from generic to specific", () => {
    expect(savedViewQueryKeys.list(workspaceId)).toEqual(["savedViews", workspaceId, "list"]);
    expect(savedViewQueryKeys.detail(workspaceId, viewId)).toEqual(["savedViews", workspaceId, "detail", viewId]);
  });
});
