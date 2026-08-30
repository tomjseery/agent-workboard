import { memo } from "react";

import type { BoardCardProjection } from "../../../core/generated";

interface BoardCardProps {
  card: BoardCardProjection;
  selected: boolean;
  focused: boolean;
  onSelect(): void;
  onFocus(): void;
  onOpen(): void;
  onKeyDown(event: React.KeyboardEvent<HTMLButtonElement>): void;
}

export const BoardCard = memo(function BoardCard({ card, selected, focused, onSelect, onFocus, onOpen, onKeyDown }: BoardCardProps) {
  return (
    <article role="listitem" aria-posinset={card.lanePosition} aria-setsize={card.laneCount} className="px-2 py-1">
      <button
        type="button"
        data-board-card={card.workItem.id}
        tabIndex={focused ? 0 : -1}
        aria-current={selected ? "true" : undefined}
        aria-label={`${card.workItem.key}: ${card.workItem.title}. Position ${card.lanePosition} of ${card.laneCount} in ${card.laneKey}.`}
        className="w-full rounded-xl border border-[var(--border)] bg-[var(--surface)] p-3 text-left focus-visible:ring-2 focus-visible:ring-[var(--accent)]"
        onClick={onSelect}
        onDoubleClick={onOpen}
        onFocus={onFocus}
        onKeyDown={onKeyDown}
      >
        <span className="block text-xs font-semibold text-[var(--accent)]">{card.workItem.key}</span>
        <span className="mt-1 block font-medium">{card.workItem.title}</span>
        <span className="mt-1 block text-xs text-[var(--muted-text)]">{card.feature.title}</span>
        <span className="mt-3 flex flex-wrap gap-1">
          {card.repositories.map((repository) => <span key={repository.id} className="rounded border border-[var(--border)] px-1.5 py-0.5 text-xs">{repository.slug}</span>)}
        </span>
        <span className="mt-2 block text-xs">Dependencies: {card.dependencyReadiness.replaceAll("_", " ")}</span>
        {card.blockedBy.length > 0 && <span className="mt-1 block text-xs text-[var(--warning)]">Blocked by {card.blockedBy.map((evidence) => evidence.workItem.key).join(", ")}</span>}
        <span className="mt-1 block text-xs">Parallel: {card.parallelReadiness.readyCount} ready, {card.parallelReadiness.waitingCount} waiting</span>
        <span className="mt-1 block text-xs">Sessions: {card.sessionSummary.total}</span>
        {card.attentionReasons.length > 0 && <span className="mt-2 block text-xs font-semibold text-[var(--warning)]">{card.attentionReasons.map((reason) => reason.message).join(" · ")}</span>}
      </button>
    </article>
  );
});
