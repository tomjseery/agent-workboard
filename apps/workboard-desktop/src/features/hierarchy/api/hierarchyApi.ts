import { daemon } from "../../../core/daemon";
import type { WorkspaceId } from "../../../core/generated";

const hierarchyApi = {
  get: (workspaceId: WorkspaceId) => daemon.workspaceHierarchy(workspaceId),
};

export default hierarchyApi;
