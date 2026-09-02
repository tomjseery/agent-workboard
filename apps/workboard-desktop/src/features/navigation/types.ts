import type { EpicId, FeatureId, RepositoryId, WorkItemStatus } from "../../core/contracts";

export interface NavigationFeatureNode {
  id: FeatureId;
  nodeId: string;
  title: string;
  slug: string;
  workItemCount: number;
  statuses: WorkItemStatus[];
}

export interface NavigationEpicNode {
  id: EpicId;
  nodeId: string;
  title: string;
  slug: string;
  features: NavigationFeatureNode[];
}

export interface NavigationRepositoryNode {
  id: RepositoryId | null;
  nodeId: string;
  title: string;
  slug: string;
  epics: NavigationEpicNode[];
}

export interface NavigationTree {
  repositories: NavigationRepositoryNode[];
  featureCount: number;
}

export interface NavigationPath {
  repositoryIds: RepositoryId[];
  epicId?: EpicId;
  featureId?: FeatureId;
}
