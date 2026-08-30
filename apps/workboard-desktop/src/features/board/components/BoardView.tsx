import type { WorkItemId, WorkspaceId } from "../../../core/generated";
import { useBoard } from "../hooks/useBoard";
import { BoardFilters } from "./BoardFilters";
import { VirtualLane } from "./VirtualLane";

interface BoardViewProps {
  workspaceId: WorkspaceId;
  onOpenWorkItem(workItemId: WorkItemId): void;
}

export function BoardView({ workspaceId, onOpenWorkItem }: BoardViewProps) {
  const board = useBoard(workspaceId);
  if (board.isLoading) return <p role="status">Loading authoritative board…</p>;
  if (board.isTransportError) return <p role="alert">The board could not be reached. No local board has been inferred.</p>;
  if (board.error?.code === "projection_version_unavailable" || board.error?.code === "incompatible_protocol") return <p role="alert">This daemon does not provide a compatible board projection. Desktop remains read-only.</p>;
  if (board.error != null) return <p role="alert">The authoritative board is unavailable: {board.error.message}</p>;

  return (
    <div className="space-y-4">
      <BoardFilters filters={board.filters} repositories={board.repositories} onQueryChange={board.setQuery} onToggleRepository={board.toggleRepository} onToggleStatus={board.toggleStatus} onSort={board.setSort} onReset={board.resetFilters} />
      <fieldset className="flex flex-wrap gap-3"><legend className="text-sm font-semibold">Visible lanes</legend>{board.lanes.map((lane) => <label key={lane.key} className="flex items-center gap-1 text-sm"><input type="checkbox" checked={board.filters.laneKeys.length === 0 || board.filters.laneKeys.includes(lane.key)} onChange={() => board.toggleLane(lane.key)} />{lane.title}</label>)}</fieldset>
      {board.isRefreshing && <p role="status">Refreshing changed board data…</p>}
      {board.isPartial && <p role="alert">Some authoritative board evidence is partial. Displayed cards retain their reported state.</p>}
      {board.totalCount === 0 ? <p>No Work items match this board view.</p> : <div aria-label="Work item board" className="flex gap-4 overflow-x-auto pb-3">{board.visibleLanes.map((lane) => <VirtualLane key={lane.key} lane={lane} cards={board.cardsByLane.get(lane.key) ?? []} selectedWorkItemId={board.selectedWorkItemId} focusedWorkItemId={board.focusedWorkItemId} onSelect={(card) => board.select(card.workItem.id)} onFocus={(card) => board.focus(card.workItem.id)} onOpen={(card) => onOpenWorkItem(card.workItem.id)} onMove={board.move} />)}</div>}
      {board.hasMore && <button type="button" disabled={board.isLoadingMore} onClick={() => void board.loadMore()} className="rounded-lg border border-[var(--border)] px-4 py-2">{board.isLoadingMore ? "Loading more cards…" : `Load more of ${board.totalCount}`}</button>}
      <p className="sr-only" aria-live="polite">{board.selectedWorkItemId === undefined ? "No card selected" : `Selected Work item ${board.selectedWorkItemId}`}</p>
    </div>
  );
}
