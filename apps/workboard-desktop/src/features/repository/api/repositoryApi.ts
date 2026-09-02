import { daemon } from "../../../core/daemon";
import type { RepositoryId, WorkspaceId } from "../../../core/contracts";

const repositoryApi = {
  get: (workspaceId: WorkspaceId, repositoryId: RepositoryId) => daemon.repositoryObservability(workspaceId, repositoryId),
};

export default repositoryApi;
