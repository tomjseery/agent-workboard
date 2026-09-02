import { useId } from "react";

import { Button } from "../../../components/ui/button";
import { Card } from "../../../components/ui/card";
import { Checkbox } from "../../../components/ui/checkbox";
import { Input } from "../../../components/ui/input";
import { Label } from "../../../components/ui/label";
import { Select } from "../../../components/ui/select";
import type { BoardViewSortDirection, BoardViewSortField, RepositoryReference } from "../../../core/contracts";
import type { BoardFilters as BoardFilterValues } from "../types";
import { laneOrder, lanePresentations } from "../model/presentation";

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
  const searchId = useId();
  const sortId = useId();
  const groupId = useId();

  return (
    <Card asChild size="compact" className="rounded-2xl p-4">
      <section aria-label="Board filters" className="space-y-3">
        <div className="flex flex-wrap items-end gap-3">
          <div className="flex min-w-64 flex-1 flex-col gap-1">
            <Label htmlFor={searchId}>Search this board</Label>
            <Input id={searchId} value={filters.query} onChange={(event) => onQueryChange(event.currentTarget.value)} />
          </div>
          <div className="flex flex-col gap-1">
            <Label htmlFor={sortId}>Sort</Label>
            <Select
              id={sortId}
              value={`${filters.sort.field}:${filters.sort.direction}`}
              onChange={(event) => {
                const [field, direction] = event.currentTarget.value.split(":") as [BoardViewSortField, BoardViewSortDirection];
                onSort(field, direction);
              }}
            >
              <option value="key:ascending">Key ascending</option>
              <option value="key:descending">Key descending</option>
              <option value="title:ascending">Title ascending</option>
              <option value="title:descending">Title descending</option>
            </Select>
          </div>
          <Button type="button" onClick={onReset}>Reset</Button>
        </div>
        <fieldset>
          <legend className="text-sm font-semibold">Lanes</legend>
          <div className="mt-2 flex flex-wrap gap-3">
            {laneOrder.map((laneKey) => (
              <div key={laneKey} className="flex items-center gap-1.5">
                <Checkbox id={`${groupId}-lane-${laneKey}`} checked={filters.laneKeys.includes(laneKey)} onCheckedChange={() => onToggleLane(laneKey)} />
                <Label htmlFor={`${groupId}-lane-${laneKey}`}>{lanePresentations[laneKey].title}</Label>
              </div>
            ))}
          </div>
        </fieldset>
        {!repositoryScoped && repositories.length > 0 && (
          <fieldset>
            <legend className="text-sm font-semibold">Repositories</legend>
            <div className="mt-2 flex max-h-24 flex-wrap gap-2 overflow-auto">
              {repositories.map((repository) => (
                <div key={repository.id} className="flex items-center gap-1.5">
                  <Checkbox id={`${groupId}-repository-${repository.id}`} checked={filters.repositoryIds.includes(repository.id)} onCheckedChange={() => onToggleRepository(repository.id)} />
                  <Label htmlFor={`${groupId}-repository-${repository.id}`}>{repository.slug}</Label>
                </div>
              ))}
            </div>
          </fieldset>
        )}
      </section>
    </Card>
  );
}
