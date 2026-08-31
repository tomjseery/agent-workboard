import { daemon } from "../../../core/daemon";
import type { WorkItemId, WorkspaceId } from "../../../core/generated";

const workItemApi = {
  detail: (workspaceId: WorkspaceId, workItemId: WorkItemId) => daemon.workItemDetail(workspaceId, workItemId),
};

export default workItemApi;
