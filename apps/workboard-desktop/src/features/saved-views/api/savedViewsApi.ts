import { daemon } from "../../../core/daemon";
import type { BoardViewDefinition, BoardViewId, WorkspaceId } from "../../../core/contracts";

const savedViewsApi = {
  list: (workspaceId: WorkspaceId) => daemon.boardViews(workspaceId),
  get: (workspaceId: WorkspaceId, viewId: BoardViewId) => daemon.boardView(workspaceId, viewId),
  save: (workspaceId: WorkspaceId, expectedRevision: number, definition: BoardViewDefinition) => daemon.execute({
    workspaceId,
    expectedRevision,
    idempotencyKey: crypto.randomUUID(),
    command: { type: "save_board_view", value: { definition } },
  }),
};

export default savedViewsApi;
