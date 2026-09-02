import { Link, Outlet } from "@tanstack/react-router";

import { Badge } from "../components/ui/badge";
import { navTabProps } from "../components/ui/nav-tabs";
import type { WorkspaceId } from "../core/generated";
import { WorkspaceSidebar } from "../features/navigation/components/WorkspaceSidebar";
import { mainContentId, useMainContentFocus } from "./useMainContentFocus";

export function AppLayout({ workspaceId }: { workspaceId: WorkspaceId }) {
  useMainContentFocus();
  return (
    <div className="min-h-screen bg-background text-foreground">
      <header className="border-b border-border bg-card">
        <div className="flex flex-wrap items-center gap-4 px-5 py-3">
          <Link to="/workspaces/$workspaceId" params={{ workspaceId }} className="text-lg font-semibold">Agent Workboard</Link>
          <nav aria-label="Primary" className="flex gap-1">
            <Link to="/workspaces/$workspaceId/board" params={{ workspaceId }} {...navTabProps("compact")}>Board</Link>
            <Link to="/workspaces/$workspaceId/attention" params={{ workspaceId }} {...navTabProps("compact")}>What needs me</Link>
            <Link to="/workspaces/$workspaceId/proposals" params={{ workspaceId }} {...navTabProps("compact")}>Proposals</Link>
          </nav>
          <Badge tone="positive" size="lg" className="ml-auto font-semibold">Daemon connected</Badge>
        </div>
      </header>
      <div className="flex items-start">
        <WorkspaceSidebar workspaceId={workspaceId} />
        <main id={mainContentId} tabIndex={-1} className="min-w-0 flex-1 px-5 py-6">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
