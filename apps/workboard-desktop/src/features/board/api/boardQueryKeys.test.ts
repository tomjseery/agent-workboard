import { expect, it } from "vitest";

import { boardQueryKeys } from "./boardQueryKeys";

it("orders canonical board keys from Workspace to exact projection parameters", () => {
  const workspaceId = "20000000-0000-0000-0000-000000000001";
  const parameters = { limit: 200, query: null, repositoryIds: [], statuses: [], laneKeys: [], sort: { field: "key" as const, direction: "ascending" as const } };
  expect(boardQueryKeys.board(workspaceId, parameters)).toEqual(["delivery-board", workspaceId, "board", parameters]);
  expect(boardQueryKeys.attention(workspaceId)).toEqual(["delivery-board", workspaceId, "attention"]);
});
