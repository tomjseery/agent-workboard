import { daemon } from "../../../core/daemon";
import type { WorkspaceId } from "../../../core/contracts";

const workspaceApi = {
  get: (workspaceId: WorkspaceId) => daemon.workspaceSummary(workspaceId),
};

export default workspaceApi;
