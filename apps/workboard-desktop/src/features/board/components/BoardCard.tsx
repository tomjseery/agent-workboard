import { memo } from "react";
import { Link } from "@tanstack/react-router";

import type { BoardCardProjection, WorkspaceId } from "../../../core/generated";

interface BoardCardProps {
  card: BoardCardProjection;
  workspaceId: WorkspaceId;
  evidenceLinks: boolean;
  selected: boolean;
  focused: boolean;
  onSelect(): void;
  onFocus(): void;
  onOpen(): void;
  onKeyDown(event: React.KeyboardEvent<HTMLButtonElement>): void;
}

export const BoardCard = memo(function BoardCard({ card, workspaceId, evidenceLinks, selected, focused, onSelect, onFocus, onOpen, onKeyDown }: BoardCardProps) {
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
      {evidenceLinks && (card.checkoutIds.length > 0 || card.sessionIds.length > 0) && <nav aria-label={`Evidence for ${card.workItem.key}`} className="mt-1 flex flex-wrap gap-2 px-1 text-xs">
        {card.checkoutIds.map((checkoutId) => <Link key={checkoutId} to="/workspaces/$workspaceId/checkouts/$checkoutId" params={{ workspaceId, checkoutId }} className="rounded border border-[var(--border)] px-2 py-1">Checkout {checkoutId.slice(0, 8)}</Link>)}
        {card.sessionIds.map((sessionId) => <Link key={sessionId} to="/workspaces/$workspaceId/sessions/$sessionId" params={{ workspaceId, sessionId }} className="rounded border border-[var(--border)] px-2 py-1">Session {sessionId.slice(0, 8)}</Link>)}
      </nav>}
    </article>
  );
});
