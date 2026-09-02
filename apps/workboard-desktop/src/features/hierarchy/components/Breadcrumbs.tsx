import { Link } from "@tanstack/react-router";
import type { ReactNode } from "react";

import type { WorkspaceHierarchy, WorkspaceId } from "../../../core/generated";
import { breadcrumbTrail, type BreadcrumbStep, type BreadcrumbTarget } from "../model/breadcrumbTrail";

interface BreadcrumbsProps {
  workspaceId: WorkspaceId;
  hierarchy: WorkspaceHierarchy;
  target: BreadcrumbTarget;
}

const stepLinks: Record<BreadcrumbStep["kind"], (workspaceId: WorkspaceId, step: BreadcrumbStep) => ReactNode> = {
  workspace: (workspaceId, step) => <Link to="/workspaces/$workspaceId" params={{ workspaceId }}>{step.title}</Link>,
  repository: (workspaceId, step) => <Link to="/workspaces/$workspaceId/repositories/$repositoryId" params={{ workspaceId, repositoryId: step.id }}>{step.title}</Link>,
  epic: (workspaceId, step) => <Link to="/workspaces/$workspaceId/epics/$epicId" params={{ workspaceId, epicId: step.id }}>{step.title}</Link>,
  feature: (workspaceId, step) => <Link to="/workspaces/$workspaceId/features/$featureId" params={{ workspaceId, featureId: step.id }}>{step.title}</Link>,
  work_item: (_workspaceId, step) => <span aria-current="page">{step.title}</span>,
};

export function Breadcrumbs({ workspaceId, hierarchy, target }: BreadcrumbsProps) {
  const steps = breadcrumbTrail(hierarchy, target);
  return (
    <nav aria-label="Breadcrumbs">
      <ol className="flex flex-wrap items-center gap-2 text-sm text-[var(--muted-text)]">
        {steps.map((step, index) => <li key={`${step.kind}-${step.id}`} className={index === 0 ? undefined : "before:mr-2 before:content-['/']"}>{stepLinks[step.kind](workspaceId, step)}</li>)}
      </ol>
    </nav>
  );
}
