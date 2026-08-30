import { Link } from "@tanstack/react-router";
import type { ReactNode } from "react";

import type { WorkspaceId } from "../../../core/generated";
import type { HierarchyEntityModel } from "../types/hierarchy";

interface EntityLinkProps {
  entity: HierarchyEntityModel;
  workspaceId: WorkspaceId;
  children?: ReactNode;
}

const entityLinks: Record<HierarchyEntityModel["kind"], (props: EntityLinkProps) => ReactNode> = {
  repository: ({ entity, workspaceId, children }) => <Link to="/workspaces/$workspaceId/repositories/$repositoryId" params={{ workspaceId, repositoryId: entity.id }} search={{ q: "" }}>{children ?? entity.title}</Link>,
  epic: ({ entity, workspaceId, children }) => <Link to="/workspaces/$workspaceId/epics/$epicId" params={{ workspaceId, epicId: entity.id }} search={{ q: "" }}>{children ?? entity.title}</Link>,
  feature: ({ entity, workspaceId, children }) => <Link to="/workspaces/$workspaceId/features/$featureId" params={{ workspaceId, featureId: entity.id }} search={{ q: "" }}>{children ?? entity.title}</Link>,
  work_item: ({ entity, workspaceId, children }) => <Link to="/workspaces/$workspaceId/work-items/$workItemId" params={{ workspaceId, workItemId: entity.id }} search={{ q: "" }}>{children ?? entity.title}</Link>,
};

export function EntityLink(props: EntityLinkProps) {
  return entityLinks[props.entity.kind](props);
}
