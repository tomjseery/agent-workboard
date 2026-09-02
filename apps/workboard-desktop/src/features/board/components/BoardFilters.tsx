import type { BoardViewSortDirection, BoardViewSortField, RepositoryReference } from "../../../core/generated";
import type { BoardFilters as BoardFilterValues } from "../types/board";
import { laneOrder, lanePresentations } from "../types/presentation";

interface BoardFiltersProps {
  filters: BoardFilterValues;
  repositories: RepositoryReference[];
  repositoryScoped: boolean;
  onQueryChange(value: string): void;
  onToggleRepository(id: RepositoryReference["id"]): void;
  onToggleLane(laneKey: (typeof laneOrder)[number]): void;
  onSort(field: BoardViewSortField, direction: BoardViewSortDirection): void;
  onReset(): void;
}

export function BoardFilters({ filters, repositories, repositoryScoped, onQueryChange, onToggleRepository, onToggleLane, onSort, onReset }: BoardFiltersProps) {
  return (
    <section aria-label="Board filters" className="space-y-3 rounded-2xl border border-[var(--border)] bg-[var(--surface)] p-4">
      <div className="flex flex-wrap gap-3"><label className="flex min-w-64 flex-1 flex-col gap-1 text-sm">Search this board<input value={filters.query} onChange={(event) => onQueryChange(event.currentTarget.value)} className="rounded-lg border border-[var(--border)] bg-[var(--canvas)] px-3 py-2" /></label><label className="flex flex-col gap-1 text-sm">Sort<select value={`${filters.sort.field}:${filters.sort.direction}`} onChange={(event) => { const [field, direction] = event.currentTarget.value.split(":") as [BoardViewSortField, BoardViewSortDirection]; onSort(field, direction); }}><option value="key:ascending">Key ascending</option><option value="key:descending">Key descending</option><option value="title:ascending">Title ascending</option><option value="title:descending">Title descending</option></select></label><button type="button" onClick={onReset} className="self-end rounded-lg border border-[var(--border)] px-3 py-2">Reset</button></div>
      <fieldset><legend className="text-sm font-semibold">Lanes</legend><div className="mt-2 flex flex-wrap gap-3">{laneOrder.map((laneKey) => <label key={laneKey} className="flex items-center gap-1 text-sm"><input type="checkbox" checked={filters.laneKeys.includes(laneKey)} onChange={() => onToggleLane(laneKey)} />{lanePresentations[laneKey].title}</label>)}</div></fieldset>
      {!repositoryScoped && repositories.length > 0 && <fieldset><legend className="text-sm font-semibold">Repositories</legend><div className="mt-2 flex max-h-24 flex-wrap gap-2 overflow-auto">{repositories.map((repository) => <label key={repository.id} className="flex items-center gap-1 text-sm"><input type="checkbox" checked={filters.repositoryIds.includes(repository.id)} onChange={() => onToggleRepository(repository.id)} />{repository.slug}</label>)}</div></fieldset>}
    </section>
  );
}
