import { Link } from "@tanstack/react-router";

import type { WorkspaceId } from "../../../core/generated";
import { useWorkspaceNavigation } from "../hooks/useWorkspaceNavigation";

const activeClasses = "bg-[var(--accent-surface)] text-[var(--accent)]";
const rowClasses = "flex-1 truncate rounded-md px-2 py-1 text-left";

function Disclosure({ expanded, label, onToggle }: { expanded: boolean; label: string; onToggle(): void }) {
  return <button type="button" aria-expanded={expanded} aria-label={`${expanded ? "Collapse" : "Expand"} ${label}`} onClick={onToggle} className="w-5 shrink-0 rounded text-xs text-[var(--muted-text)]">{expanded ? "▾" : "▸"}</button>;
}

export function WorkspaceSidebar({ workspaceId }: { workspaceId: WorkspaceId }) {
  const navigation = useWorkspaceNavigation(workspaceId);
  return (
    <aside aria-label="Workspace navigation" className="w-72 shrink-0 border-r border-[var(--border)] bg-[var(--surface)]">
      <div className="sticky top-0 max-h-screen overflow-y-auto p-3">
        <label className="grid gap-1 text-xs"><span className="font-semibold uppercase tracking-wider text-[var(--muted-text)]">Filter navigation</span><input value={navigation.filter} onChange={(event) => navigation.setFilter(event.currentTarget.value)} className="rounded-lg border border-[var(--border)] bg-[var(--canvas)] px-2 py-1.5 text-sm" /></label>
        {navigation.isLoading && <p role="status" className="mt-4 text-sm">Loading navigation…</p>}
        {!navigation.isLoading && navigation.isUnavailable && <p role="alert" className="mt-4 text-sm">The authoritative hierarchy is unavailable.</p>}
        {navigation.tree !== undefined && navigation.tree.repositories.length === 0 && <p className="mt-4 text-sm text-[var(--muted-text)]">No repositories match this filter.</p>}
        {navigation.tree !== undefined && navigation.tree.repositories.length > 0 && (
          <ul className="mt-3 space-y-1 text-sm">
            {navigation.tree.repositories.map((repository) => {
              const expanded = navigation.isExpanded(repository.nodeId, "repository");
              return (
                <li key={repository.nodeId}>
                  <div className="flex items-center gap-1">
                    <Disclosure expanded={expanded} label={repository.title} onToggle={() => navigation.toggle(repository.nodeId, "repository")} />
                    {repository.id === null ? <span className={`${rowClasses} text-[var(--muted-text)]`}>{repository.title}</span> : <Link to="/workspaces/$workspaceId/repositories/$repositoryId" params={{ workspaceId, repositoryId: repository.id }} className={`${rowClasses} font-medium`} activeProps={{ className: `${rowClasses} font-medium ${activeClasses}`, "aria-current": "page" }}>{repository.title}</Link>}
                  </div>
                  {expanded && (
                    <ul className="ml-4 mt-1 space-y-1 border-l border-[var(--border)] pl-2">
                      {repository.epics.map((epic) => {
                        const epicExpanded = navigation.isExpanded(epic.nodeId, "epic");
                        return (
                          <li key={epic.nodeId}>
                            <div className="flex items-center gap-1">
                              <Disclosure expanded={epicExpanded} label={epic.title} onToggle={() => navigation.toggle(epic.nodeId, "epic")} />
                              <Link to="/workspaces/$workspaceId/epics/$epicId" params={{ workspaceId, epicId: epic.id }} className={rowClasses} activeProps={{ className: `${rowClasses} ${activeClasses}`, "aria-current": "page" }}>{epic.title}</Link>
                            </div>
                            {epicExpanded && (
                              <ul className="ml-4 mt-1 space-y-1 border-l border-[var(--border)] pl-2">
                                {epic.features.length === 0 && <li className="px-2 py-1 text-xs text-[var(--muted-text)]">No Features recorded</li>}
                                {epic.features.map((feature) => (
                                  <li key={feature.nodeId} className="flex items-center gap-1">
                                    <Link to="/workspaces/$workspaceId/features/$featureId" params={{ workspaceId, featureId: feature.id }} className={rowClasses} activeProps={{ className: `${rowClasses} ${activeClasses}`, "aria-current": "page" }}>{feature.title}</Link>
                                    <span aria-label={`${feature.workItemCount} Work items`} className="shrink-0 rounded-full border border-[var(--border)] px-1.5 text-xs text-[var(--muted-text)]">{feature.workItemCount}</span>
                                  </li>
                                ))}
                              </ul>
                            )}
                          </li>
                        );
                      })}
                    </ul>
                  )}
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </aside>
  );
}
