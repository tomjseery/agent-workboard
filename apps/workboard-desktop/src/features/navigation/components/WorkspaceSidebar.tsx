import { Link } from "@tanstack/react-router";
import { useId } from "react";

import { Badge } from "../../../components/ui/badge";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "../../../components/ui/collapsible";
import { Input } from "../../../components/ui/input";
import { Label } from "../../../components/ui/label";
import type { WorkspaceId } from "../../../core/contracts";
import { cn } from "../../../lib/utils";
import { useWorkspaceNavigation } from "../hooks/useWorkspaceNavigation";

const rowClass = "flex-1 truncate rounded-md px-2 py-1 text-left";
const rowActiveClass = "bg-accent text-accent-foreground";

export function WorkspaceSidebar({ workspaceId }: { workspaceId: WorkspaceId }) {
  const navigation = useWorkspaceNavigation(workspaceId);
  const filterId = useId();

  return (
    <aside aria-label="Workspace navigation" className="w-72 max-w-[45%] shrink-0 border-r border-border bg-card">
      <div className="sticky top-0 max-h-screen overflow-y-auto p-3">
        <div className="grid gap-1">
          <Label htmlFor={filterId} className="text-xs font-semibold tracking-wider text-muted-foreground uppercase">Filter navigation</Label>
          <Input id={filterId} value={navigation.filter} onChange={(event) => navigation.setFilter(event.currentTarget.value)} className="px-2 py-1.5" />
        </div>
        {navigation.isLoading && <p role="status" className="mt-4 text-sm">Loading navigation…</p>}
        {!navigation.isLoading && navigation.isUnavailable && <p role="alert" className="mt-4 text-sm">The authoritative hierarchy is unavailable.</p>}
        {navigation.tree !== undefined && navigation.tree.repositories.length === 0 && <p className="mt-4 text-sm text-muted-foreground">No repositories match this filter.</p>}
        {navigation.tree !== undefined && navigation.tree.repositories.length > 0 && (
          <ul className="mt-3 space-y-1 text-sm">
            {navigation.tree.repositories.map((repository) => {
              const expanded = navigation.isExpanded(repository.nodeId, "repository");
              return (
                <li key={repository.nodeId}>
                  <Collapsible open={expanded} onOpenChange={() => navigation.toggle(repository.nodeId, "repository")}>
                    <div className="flex items-center gap-1">
                      <CollapsibleTrigger aria-label={`${expanded ? "Collapse" : "Expand"} ${repository.title}`} />
                      {repository.id === null ? (
                        <span className={cn(rowClass, "text-muted-foreground")}>{repository.title}</span>
                      ) : (
                        <Link
                          to="/workspaces/$workspaceId/repositories/$repositoryId"
                          params={{ workspaceId, repositoryId: repository.id }}
                          className={cn(rowClass, "font-medium")}
                          activeProps={{ className: cn(rowClass, "font-medium", rowActiveClass), "aria-current": "page" }}
                        >
                          {repository.title}
                        </Link>
                      )}
                    </div>
                    <CollapsibleContent>
                      <ul className="mt-1 ml-4 space-y-1 border-l border-border pl-2">
                        {repository.epics.map((epic) => {
                          const epicExpanded = navigation.isExpanded(epic.nodeId, "epic");
                          return (
                            <li key={epic.nodeId}>
                              <Collapsible open={epicExpanded} onOpenChange={() => navigation.toggle(epic.nodeId, "epic")}>
                                <div className="flex items-center gap-1">
                                  <CollapsibleTrigger aria-label={`${epicExpanded ? "Collapse" : "Expand"} ${epic.title}`} />
                                  <Link
                                    to="/workspaces/$workspaceId/epics/$epicId"
                                    params={{ workspaceId, epicId: epic.id }}
                                    className={rowClass}
                                    activeProps={{ className: cn(rowClass, rowActiveClass), "aria-current": "page" }}
                                  >
                                    {epic.title}
                                  </Link>
                                </div>
                                <CollapsibleContent>
                                  <ul className="mt-1 ml-4 space-y-1 border-l border-border pl-2">
                                    {epic.features.length === 0 && <li className="px-2 py-1 text-xs text-muted-foreground">No Features recorded</li>}
                                    {epic.features.map((feature) => (
                                      <li key={feature.nodeId} className="flex items-center gap-1">
                                        <Link
                                          to="/workspaces/$workspaceId/features/$featureId"
                                          params={{ workspaceId, featureId: feature.id }}
                                          className={rowClass}
                                          activeProps={{ className: cn(rowClass, rowActiveClass), "aria-current": "page" }}
                                        >
                                          {feature.title}
                                        </Link>
                                        <Badge tone="muted" size="tag" aria-label={`${feature.workItemCount} Work items`} className="shrink-0 rounded-full">
                                          {feature.workItemCount}
                                        </Badge>
                                      </li>
                                    ))}
                                  </ul>
                                </CollapsibleContent>
                              </Collapsible>
                            </li>
                          );
                        })}
                      </ul>
                    </CollapsibleContent>
                  </Collapsible>
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </aside>
  );
}
