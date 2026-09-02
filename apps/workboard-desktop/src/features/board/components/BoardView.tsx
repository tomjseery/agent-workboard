import { Alert } from "../../../components/ui/alert";
import { Button } from "../../../components/ui/button";
import type { WorkItemId, WorkspaceId } from "../../../core/contracts";
import { useBoard } from "../hooks/useBoard";
import type { BoardScope } from "../types";
import { BoardFilters } from "./BoardFilters";
import { VirtualLane } from "./VirtualLane";

interface BoardViewProps {
  workspaceId: WorkspaceId;
  scope?: BoardScope;
  evidenceLinks?: boolean;
  onOpenWorkItem(workItemId: WorkItemId): void;
}

export function BoardView({ workspaceId, scope, evidenceLinks = false, onOpenWorkItem }: BoardViewProps) {
  const board = useBoard(workspaceId, scope);
  if (board.isLoading) return <p role="status">Loading authoritative board…</p>;
  if (board.isTransportError) return <p role="alert">The board could not be reached. No local board has been inferred.</p>;
  if (board.error?.code === "projection_version_unavailable" || board.error?.code === "incompatible_protocol") return <p role="alert">This daemon does not provide a compatible board projection. Desktop remains read-only.</p>;
  if (board.error != null) return <p role="alert">The authoritative board is unavailable: {board.error.message}</p>;

  return (
    <div className="space-y-4">
      <BoardFilters filters={board.filters} repositories={board.repositories} repositoryScoped={board.isRepositoryScoped} onQueryChange={board.setQuery} onToggleRepository={board.toggleRepository} onToggleLane={board.toggleLane} onSort={board.setSort} onReset={board.resetFilters} />
      {board.isRefreshing && <p role="status">Refreshing changed board data…</p>}
      {board.isPartial && <Alert>Some authoritative board evidence is partial. Displayed cards retain their reported state.</Alert>}
      {board.totalCount === 0 ? (
        <p>No Work items match this board view.</p>
      ) : (
        <div aria-label="Work item board" className="flex gap-4 overflow-x-auto pb-3">
          {board.lanes.map((lane) => (
            <VirtualLane
              key={lane.key}
              lane={lane}
              workspaceId={workspaceId}
              evidenceLinks={evidenceLinks}
              cards={board.cardsByLane.get(lane.key) ?? []}
              selectedWorkItemId={board.selectedWorkItemId}
              focusedWorkItemId={board.focusedWorkItemId}
              onSelect={(card) => board.select(card.workItem.id)}
              onFocus={(card) => board.focus(card.workItem.id)}
              onOpen={(card) => onOpenWorkItem(card.workItem.id)}
              onMove={board.move}
            />
          ))}
        </div>
      )}
      {board.hasMore && (
        <Button type="button" size="lg" disabled={board.isLoadingMore} onClick={() => void board.loadMore()}>
          {board.isLoadingMore ? "Loading more cards…" : `Load more of ${board.totalCount}`}
        </Button>
      )}
      <p className="sr-only" aria-live="polite">{board.selectedWorkItemId === undefined ? "No card selected" : `Selected Work item ${board.selectedWorkItemId}`}</p>
    </div>
  );
}
