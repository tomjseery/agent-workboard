import type { AttentionQuery, BoardQuery, WorkspaceId } from "../../../core/generated";

type BoardParameters = Omit<BoardQuery, "cursor">;
type AttentionParameters = Omit<AttentionQuery, "cursor">;

export const boardQueryKeys = {
  all: ["delivery-board"] as const,
  workspace: (workspaceId: WorkspaceId) => [...boardQueryKeys.all, workspaceId] as const,
  boards: (workspaceId: WorkspaceId) => [...boardQueryKeys.workspace(workspaceId), "board"] as const,
  board: (workspaceId: WorkspaceId, parameters: BoardParameters) => [...boardQueryKeys.boards(workspaceId), parameters] as const,
  attention: (workspaceId: WorkspaceId) => [...boardQueryKeys.workspace(workspaceId), "attention"] as const,
  attentionList: (workspaceId: WorkspaceId, parameters: AttentionParameters) => [...boardQueryKeys.attention(workspaceId), parameters] as const,
};
