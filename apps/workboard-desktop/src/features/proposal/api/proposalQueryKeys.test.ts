import { describe, expect, it } from "vitest";

import { proposalQueryKeys } from "./proposalQueryKeys";

describe("proposal query keys", () => {
  it("scopes queue and canonical detail independently", () => {
    const workspaceId = "20000000-0000-0000-0000-000000000001";
    const featureId = "50000000-0000-0000-0000-000000000001";
    expect(proposalQueryKeys.queue(workspaceId)).toEqual(["proposals", workspaceId, "approval-queue"]);
    expect(proposalQueryKeys.detail(workspaceId, featureId)).toEqual(["proposals", workspaceId, "detail", featureId]);
  });
});
