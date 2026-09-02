import { daemon } from "../../../core/daemon";
import type { AttentionQuery, BoardQuery, WorkspaceId } from "../../../core/contracts";

export const boardApi = {
  page: (workspaceId: WorkspaceId, query: BoardQuery) => daemon.board(workspaceId, query),
  attention: (workspaceId: WorkspaceId, query: AttentionQuery) => daemon.attention(workspaceId, query),
};
